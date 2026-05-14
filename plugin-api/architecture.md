# Termy Lightweight Rust Plugin API Architecture

## Objective

Design a fast, lightweight plugin API for Termy that lets users write pure Rust
plugins which add custom commands to the command palette.

This is a first-phase architecture. It prioritizes command-palette extension,
startup safety, low host overhead, and a small stable SDK surface.

## Existing Repo Fit

The current command system has a useful boundary:

- `termy_command_core` owns built-in command IDs, config names, keybind parsing,
  and availability.
- `src/commands.rs` maps built-in command IDs to GPUI actions, palette labels,
  menu labels, and platform visibility.
- `src/terminal_view/command_palette/state.rs` owns palette item state and
  filtering.
- `src/terminal_view/command_palette/mod.rs` builds command rows and dispatches
  selected rows.

Plugins should not extend `termy_command_core`. That crate should remain the
pure built-in command and keybind domain. Plugin commands are runtime
contributions owned by a new plugin domain and adapted into the palette at the
UI edge.

## Recommendation

Use out-of-process Rust plugin binaries with a tiny Rust SDK over newline
delimited JSON on stdio.

Why this shape:

- Pure Rust authoring: plugin authors write normal Rust and depend on
  `termy_plugin_sdk`.
- Lightweight host: no scripting VM and no dynamic library ABI layer.
- Safe failure mode: a plugin crash kills only the plugin process.
- Stable compatibility: protocol schema versioning is easier than Rust dynamic
  ABI compatibility.
- Fast palette: command metadata is loaded once and cached; palette filtering
  stays in-process.

Rejected for v1:

- Dynamic libraries: lower invoke overhead, but Rust ABI stability and crash
  isolation are poor.
- WASM: good sandboxing, but adds runtime complexity before the API needs it.
- Embedded scripting: not pure Rust and adds a second ecosystem to support.

## Crates

```text
crates/plugin_core/
  Pure shared protocol types.
  No gpui, no process spawning, no terminal internals.

crates/plugin_sdk/
  Author-facing Rust SDK.
  Depends on plugin_core, handles stdio framing and dispatch.

src/plugins/
  Host runtime inside the Termy app.
  Discovery, manifest validation, process lifecycle, command registry,
  invoke queue, logs, and permission checks.
```

`plugin_core` should be publishable later, but it can start as a workspace crate.
`plugin_sdk` should be thin enough that plugins could also implement the wire
protocol directly.

## Manifest

Use JSON to align with the existing plugin protocol docs.

```json
{
  "schema_version": 1,
  "id": "dev.tools",
  "name": "Dev Tools",
  "version": "0.1.0",
  "entrypoint": "./dev-tools",
  "contributes": {
    "commands": [
      {
        "id": "deploy_current_project",
        "title": "Deploy Current Project",
        "keywords": ["deploy", "ship", "railway"],
        "requires": ["active_tab"]
      }
    ]
  }
}
```

Rules:

- Plugin IDs are reverse-DNS-ish or dotted ASCII: `dev.tools`.
- Command IDs are plugin-local ASCII slugs: `deploy_current_project`.
- Host-visible command key is `{plugin_id}.{command_id}`.
- Duplicate plugin IDs are rejected.
- Duplicate command keys are rejected.
- Missing or invalid manifests are shown in Settings but not loaded.

## Runtime Protocol

Transport: newline delimited JSON over stdio.

Host starts the plugin only when needed unless `"autostart": true` is added
later. For command-palette v1, lazy start is enough.

Host to plugin:

```json
{"type":"hello","protocol":1,"host":"termy","host_version":"0.1.86","plugin_id":"dev.tools"}
{"type":"invoke_command","request_id":1,"command_id":"deploy_current_project","context":{"active_tab":true,"active_title":"api-server"}}
{"type":"shutdown"}
```

Plugin to host:

```json
{"type":"hello","protocol":1,"plugin_id":"dev.tools","sdk_version":"0.1.0"}
{"type":"command_result","request_id":1,"status":"ok","actions":[{"type":"write_active_terminal","text":"railway up\n"}]}
{"type":"log","level":"info","message":"deploy command invoked"}
```

Timeouts:

- Handshake: 500 ms.
- Command invoke: default 5 seconds, manifest can request up to a capped value.
- Shutdown: 250 ms before kill.

The host owns all terminal mutations. Plugins return action requests; they do
not get direct access to GPUI, terminal structs, or app state.

## Host Data Model

```rust
struct PluginRegistry {
    plugins: HashMap<PluginId, PluginRecord>,
    commands: HashMap<PluginCommandKey, PluginCommandRecord>,
}

struct PluginCommandRecord {
    plugin_id: PluginId,
    command_id: PluginCommandId,
    title: String,
    keywords: Vec<String>,
    requires: Vec<PluginCommandRequirement>,
}

enum PluginProcessState {
    Stopped,
    Starting,
    Running(PluginProcess),
    Failed { message: String },
}
```

The registry is refreshed on startup and when the user presses Refresh in
Settings. File watching can wait.

## Palette Integration

Add one runtime item kind:

```rust
CommandPaletteItemKind::PluginCommand {
    plugin_id: String,
    command_id: String,
}
```

Add `PluginRegistry::palette_items(context)` that maps command records into
existing `CommandPaletteItem` rows. `TerminalView::command_palette_items_for_mode`
then appends plugin rows in `CommandPaletteMode::Commands`:

```text
built-in command rows
plugin command rows
```

Filtering remains unchanged because plugin rows already expose title and
keywords. Keybindings remain out of scope for v1; custom commands appear only in
the command palette.

## Permission Model

Start with a deny-by-default host action allowlist.

Allowed v1 actions:

- `write_active_terminal`: write text to the active terminal.
- `open_new_tab`: create a tab with optional working directory.
- `show_toast`: show a short notification.
- `copy_to_clipboard`: write text to clipboard.

Context reads:

- `active_tab`: whether an active terminal exists.
- `active_title`: current tab title.
- `working_directory`: only if the plugin declares `read_working_directory`.

Permissions are checked twice:

1. Manifest validation decides whether a command can be enabled.
2. Action application validates every returned host action.

## SDK Surface

```rust
pub trait Plugin {
    fn commands(&self) -> Vec<Command>;
    fn run(&mut self, command_id: &str, ctx: PluginContext) -> Result<()>;
}

pub struct Command {
    pub id: &'static str,
    pub title: &'static str,
    pub keywords: Vec<&'static str>,
    pub requires: Vec<Requirement>,
}

pub struct PluginContext {
    pub request: CommandRequest,
    pub host: HostHandle,
}
```

The SDK macro:

```rust
termy_plugin_sdk::main!(MyPlugin);
```

expands to stdio setup, hello validation, command dispatch, panic-to-error
conversion, JSON framing, and shutdown handling.

## Performance Targets

- Manifest scan: no plugin processes launched.
- Palette open: no synchronous plugin IO.
- Palette row build: cached command records only.
- Command invoke: one process start at most, then one request/response.
- Long-running plugin work: plugin process owns it; host keeps UI responsive.

Practical budgets:

- 100 plugins with 10 commands each should add less than 10 ms to palette row
  construction on a warm registry.
- Plugin handshake timeout should fail fast and leave a visible Settings error.
- Command invocation should never block the GPUI event loop.

## Error Handling

- Invalid manifest: plugin appears in Settings as invalid; commands are omitted.
- Plugin handshake timeout: mark failed, toast concise message, keep logs.
- Unknown command response: ignore response and log protocol error.
- Permission violation: reject the action, mark command result failed, log detail.
- Plugin exits during invoke: toast command failure and mark plugin stopped.

## First Implementation Slices

1. Add `crates/plugin_core` with manifest and protocol types.
2. Add `src/plugins` discovery and manifest validation.
3. Add plugin command rows to `CommandPaletteItemKind` and command palette build.
4. Add process runtime with handshake, invoke, timeout, and shutdown.
5. Add `crates/plugin_sdk` plus a tiny example plugin.
6. Add Settings inspection for discovered plugins and manifest errors.
7. Add focused tests for manifest validation, registry command keys, palette
   filtering, and invoke timeout handling.

## Out Of Scope For V1

- Plugin keybindings.
- Plugin menus outside the command palette.
- Rich plugin panels.
- Marketplace or remote install.
- Hot reload file watchers.
- Direct terminal state mutation from plugin code.
- Dynamic library plugins.

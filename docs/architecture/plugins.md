# Plugin System

Termy plugins are isolated executables discovered from the Termy config directory.

## Current shape

- Plugins live under `<config-dir>/plugins/<plugin-id>/`
- Each plugin directory must contain `termy-plugin.json`
- `entrypoint` may be relative to the plugin directory or absolute
- Plugins are launched out-of-process over stdio using newline-delimited JSON messages
- Unresponsive or invalid plugins are rejected during startup handshake
- Rust plugin authors can use `termy_plugin_sdk` for the handshake and message loop

## Manifest

Example:

```json
{
  "schema_version": 1,
  "id": "example.hello",
  "name": "Hello Plugin",
  "version": "0.1.0",
  "description": "Minimal example plugin",
  "runtime": "executable",
  "entrypoint": "./plugin.sh",
  "autostart": true,
  "permissions": ["network"],
  "contributes": {
    "commands": [
      {
        "id": "example.hello.run",
        "title": "Run Hello"
      }
    ]
  }
}
```

## Handshake

Host to plugin:

```json
{"type":"hello","payload":{"protocol_version":1,"host_name":"termy","host_version":"0.1.44","plugin_id":"example.hello"}}
```

Plugin to host:

```json
{"type":"hello","payload":{"protocol_version":1,"plugin_id":"example.hello","name":"Hello Plugin","version":"0.1.0","capabilities":["command_provider"]}}
```

Shutdown:

```json
{"type":"shutdown"}
```

## SDK

`termy_plugin_sdk` currently provides:

- stdio session bootstrap
- host hello validation
- plugin hello emission
- typed receive/send helpers
- host event extraction helper
- toast helper
- panel update helper
- a `run_until_shutdown` loop

Rust example:

```rust
use termy_plugin_core::{HostRpcMessage, PluginCapability};
use termy_plugin_sdk::{PluginMetadata, PluginSession};

let metadata = PluginMetadata::new("example.hello-rust", "Hello Rust Plugin", "0.1.0")
    .with_capabilities(vec![PluginCapability::CommandProvider]);
let mut session = PluginSession::stdio(metadata)?;

session.run_until_shutdown(|message, session| {
    if matches!(message, HostRpcMessage::Ping) {
        session.send_pong()?;
    }
    Ok(())
})?;
```

Plugins can publish a lightweight settings panel by declaring the `ui_panels` permission, advertising the `ui_panel` capability, and calling:

```rust
session.send_panel("Plugin Status", "Everything is healthy")?;
```

Panels can also expose action buttons that invoke contributed plugin commands:

```rust
use termy_plugin_core::PluginPanelAction;

session.send_panel_with_actions(
    "Plugin Status",
    "Everything is healthy",
    vec![PluginPanelAction {
        command_id: "example.status.refresh".to_string(),
        label: "Refresh".to_string(),
        enabled: true,
    }],
)?;
```

## Event subscriptions

Plugins can subscribe to selected host events in the manifest:

```json
{
  "schema_version": 1,
  "id": "example.events",
  "name": "Events Plugin",
  "version": "0.1.0",
  "runtime": "executable",
  "entrypoint": "./plugin.sh",
  "permissions": ["host_events"],
  "subscribes": {
    "events": ["app_started", "theme_changed", "active_tab_changed"]
  }
}
```

Subscribed events currently available:

- `app_started`
- `theme_changed`
- `active_tab_changed`

Plugins must declare the `host_events` permission and advertise the `event_subscriber` capability during handshake if they subscribe to host events.

Rust example:

```rust
use termy_plugin_core::{HostEvent, HostRpcMessage, PluginCapability};
use termy_plugin_sdk::{PluginMetadata, PluginSession};

let metadata = PluginMetadata::new("example.events", "Events Plugin", "0.1.0")
    .with_capabilities(vec![PluginCapability::EventSubscriber]);
let mut session = PluginSession::stdio(metadata)?;

session.run_until_shutdown(|message, session| {
    if let Some(event) = PluginSession::event(message) {
        match event {
            HostEvent::AppStarted { host_version } => {
                session.send_log(termy_plugin_core::PluginLogLevel::Info, format!("host {host_version}"))?;
            }
            HostEvent::ThemeChanged { theme_id } => {
                session.send_log(termy_plugin_core::PluginLogLevel::Info, format!("theme={theme_id}"))?;
            }
            HostEvent::ActiveTabChanged { tab_index, tab_title } => {
                session.send_log(
                    termy_plugin_core::PluginLogLevel::Info,
                    format!("active tab #{tab_index}: {tab_title}"),
                )?;
            }
        }
    }
    if matches!(message, HostRpcMessage::Ping) {
        session.send_pong()?;
    }
    Ok(())
})?;
```

## First-phase capabilities

- plugin discovery
- manifest validation
- stdio process launch
- protocol version check
- command dispatch into plugins
- host event subscriptions
- settings UI panels
- runtime plugin log consumption
- startup failure isolation
- plugin shutdown on host drop
- live start/stop from the app-host runtime
- recent per-plugin log buffering for inspection

## CLI inspection

Use the CLI to inspect discovered manifests without starting the UI:

```bash
cargo run -p termy_cli -- -list-plugins
```

Create a starter plugin scaffold that already demonstrates command handling, event subscriptions, toasts, and settings panel updates:

```bash
cargo run -p termy_cli -- -plugin-init
```

A dedicated Rust reference plugin also lives in `crates/plugin_example_status/`.

A standalone installable example plugin lives in `examples/plugin-full/` and is packaged by CI as an artifact.

## Settings integration

The Settings `Plugins` tab currently supports:

- inspecting discovered plugins
- opening the plugin directory
- installing a plugin from a local folder
- removing an installed plugin
- toggling `autostart` in the manifest
- live `Start` / `Stop` for currently discovered plugins
- viewing recent runtime log lines captured by the host
- rendering the latest plugin-provided panel content for running `ui_panel` plugins

## Not implemented yet

- broader permission enforcement beyond current toast and capability gating
- native in-app registry install flows
- richer registry/marketplace moderation and discovery flows

# Tmux Integration Hardening Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make tmux integration a true add-on by fixing runtime correctness/performance risks, centralizing capability gating, and reducing cross-cutting architectural leakage.

**Architecture:** Do a hard cutover in three tracks: (1) shared command-availability domain model consumed by all surfaces, (2) tmux runtime pipeline hardening in `termy_terminal_ui`, and (3) `TerminalView` boundary cleanup so UI consumes a backend/event model instead of direct tmux orchestration. No backward-compat compatibility paths.

**Tech Stack:** Rust 2024, `gpui`, `flume`, `smol`, `termy_command_core`, `termy_terminal_ui`, `termy_cli`, `termy_native_sdk`, `xtask`.

---

### Task 1: Add Shared Command Availability Model In `command_core`

**Files:**
- Create: `crates/command_core/src/availability.rs`
- Modify: `crates/command_core/src/catalog.rs`
- Modify: `crates/command_core/src/lib.rs`
- Test: `crates/command_core/src/availability.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn command_availability_reports_requires_tmux_when_runtime_disabled() {
    let caps = CommandCapabilities {
        tmux_runtime_active: false,
        install_cli_available: true,
    };
    let availability = CommandId::SplitPaneVertical.availability(caps);
    assert!(!availability.enabled);
    assert_eq!(
        availability.reason,
        Some(CommandUnavailableReason::RequiresTmuxRuntime)
    );
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p termy_command_core command_availability_reports_requires_tmux_when_runtime_disabled -- --exact`  
Expected: FAIL with missing `CommandCapabilities`/`availability`.

**Step 3: Write minimal implementation**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandCapabilities {
    pub tmux_runtime_active: bool,
    pub install_cli_available: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandUnavailableReason {
    RequiresTmuxRuntime,
    InstallCliAlreadyInstalled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandAvailability {
    pub enabled: bool,
    pub reason: Option<CommandUnavailableReason>,
}

impl CommandId {
    pub const fn availability(self, caps: CommandCapabilities) -> CommandAvailability {
        if self.is_tmux_only() && !caps.tmux_runtime_active {
            return CommandAvailability {
                enabled: false,
                reason: Some(CommandUnavailableReason::RequiresTmuxRuntime),
            };
        }
        if matches!(self, CommandId::InstallCli) && !caps.install_cli_available {
            return CommandAvailability {
                enabled: false,
                reason: Some(CommandUnavailableReason::InstallCliAlreadyInstalled),
            };
        }
        CommandAvailability {
            enabled: true,
            reason: None,
        }
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p termy_command_core availability::tests:: -- --nocapture`  
Expected: PASS.

**Step 5: Commit**

```bash
git add crates/command_core/src/availability.rs crates/command_core/src/catalog.rs crates/command_core/src/lib.rs
git commit -m "refactor(command_core): add shared command availability model"
```

### Task 2: Route App Command Availability Through Shared Model

**Files:**
- Modify: `src/commands.rs`
- Modify: `src/terminal_view/interaction/actions.rs`
- Test: `src/commands.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn command_action_availability_reason_matches_command_core() {
    let caps = CommandCapabilities {
        tmux_runtime_active: false,
        install_cli_available: true,
    };
    let availability = CommandAction::SplitPaneVertical.availability(caps);
    assert!(!availability.enabled);
    assert_eq!(
        availability.reason,
        Some(CommandUnavailableReason::RequiresTmuxRuntime)
    );
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p termy command_action_availability_reason_matches_command_core -- --exact`  
Expected: FAIL with missing `CommandAction::availability`.

**Step 3: Write minimal implementation**

```rust
impl CommandAction {
    pub fn availability(self, caps: CommandCapabilities) -> CommandAvailability {
        self.to_command_id().availability(caps)
    }
}
```

And in `execute_command_action`, replace manual tmux guard with availability reason mapping.

**Step 4: Run test to verify it passes**

Run: `cargo test -p termy command_action_availability_reason_matches_command_core -- --exact`  
Expected: PASS.

**Step 5: Commit**

```bash
git add src/commands.rs src/terminal_view/interaction/actions.rs
git commit -m "refactor(app): consume command_core availability in command execution"
```

### Task 3: Keep Tmux Commands Visible In Palette As Disabled Rows

**Files:**
- Modify: `src/terminal_view/command_palette/mod.rs`
- Modify: `src/terminal_view/command_palette/state.rs`
- Test: `src/terminal_view/command_palette/mod.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn tmux_commands_are_present_but_disabled_when_tmux_runtime_is_off() {
    let items = TerminalView::command_palette_command_items_for_state(false, false);
    let split = items.iter().find_map(|item| match item.kind {
        CommandPaletteItemKind::Command(CommandAction::SplitPaneVertical) => Some(item),
        _ => None,
    }).expect("missing split pane command");
    assert!(!split.enabled);
    assert_eq!(split.status_hint, Some("tmux required"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p termy tmux_commands_are_present_but_disabled_when_tmux_runtime_is_off -- --exact`  
Expected: FAIL because tmux rows are currently filtered out.

**Step 3: Write minimal implementation**

```rust
fn command_palette_command_items_for_state(
    install_cli_available: bool,
    tmux_enabled: bool,
) -> Vec<CommandPaletteItem> {
    let caps = CommandCapabilities {
        tmux_runtime_active: tmux_enabled,
        install_cli_available,
    };
    CommandAction::palette_entries()
        .into_iter()
        .map(|entry| {
            let availability = entry.action.availability(caps);
            let status_hint = match availability.reason {
                Some(CommandUnavailableReason::RequiresTmuxRuntime) => Some("tmux required"),
                Some(CommandUnavailableReason::InstallCliAlreadyInstalled) => Some("Installed"),
                None => None,
            };
            CommandPaletteItem::command_with_state(
                entry.title,
                entry.keywords,
                entry.action,
                availability.enabled,
                status_hint,
            )
        })
        .collect()
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p termy command_palette::tests:: -- --nocapture`  
Expected: PASS, including updated tmux discoverability assertions.

**Step 5: Commit**

```bash
git add src/terminal_view/command_palette/mod.rs src/terminal_view/command_palette/state.rs
git commit -m "feat(palette): show tmux commands disabled with reason hints"
```

### Task 4: Unify Menus And Keybinding Install With Shared Availability

**Files:**
- Modify: `src/menus.rs`
- Modify: `src/keybindings/mod.rs`
- Test: `src/menus.rs`
- Test: `src/keybindings/mod.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn keybind_resolution_emits_tmux_suppression_warning_when_tmux_disabled() {
    let mut config = AppConfig::default();
    config.keybind_lines.push(KeybindConfigLine {
        line_number: 10,
        value: "secondary-d=split_pane_vertical".to_string(),
    });
    let (_resolved, warnings) = resolve_keybinds_for_config(&config, false);
    assert!(warnings.iter().any(|w| w.message.contains("tmux")));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p termy keybind_resolution_emits_tmux_suppression_warning_when_tmux_disabled -- --exact`  
Expected: FAIL because tmux-only keybinds are silently dropped.

**Step 3: Write minimal implementation**

```rust
if !tmux_enabled {
    let dropped_tmux = resolved.iter().filter(|binding| binding.action.is_tmux_only()).count();
    if dropped_tmux > 0 {
        warnings.push(KeybindWarning {
            line_number: 0,
            message: format!("{dropped_tmux} tmux-only keybind(s) ignored while tmux is disabled"),
        });
    }
}
```

Then update `menus.rs` title/visibility decisions to consume `CommandAction::availability(...)` instead of ad-hoc checks.

**Step 4: Run test to verify it passes**

Run: `cargo test -p termy keybindings::tests:: -- --nocapture`  
Run: `cargo test -p termy menus::tests:: -- --nocapture`  
Expected: PASS.

**Step 5: Commit**

```bash
git add src/menus.rs src/keybindings/mod.rs
git commit -m "refactor(ui): centralize menu and keybind availability decisions"
```

### Task 5: Hard-Cutover CLI Provider Output To Include Availability Metadata

**Files:**
- Modify: `crates/cli/src/commands/providers.rs`
- Modify: `crates/cli/src/commands/list_actions.rs`
- Modify: `crates/cli/src/commands/list_keybinds.rs`
- Modify: `crates/cli/src/commands/tui.rs`
- Modify: `crates/cli/src/commands/validate_config.rs`
- Test: `crates/cli/src/commands/providers.rs`
- Test: `crates/cli/src/commands/validate_config.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn list_actions_includes_tmux_metadata_when_runtime_is_disabled() {
    let lines = action_lines_for_tmux_enabled(false);
    assert!(lines.iter().any(|line| {
        line.contains("split_pane_vertical")
            && line.contains("tmux_required=true")
            && line.contains("available=false")
    }));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p termy_cli list_actions_includes_tmux_metadata_when_runtime_is_disabled -- --exact`  
Expected: FAIL because tmux-only actions are currently removed from CLI output.

**Step 3: Write minimal implementation**

```rust
format!(
    "{}\tavailable={}\ttmux_required={}\trestart_required={}",
    id.config_name(),
    availability.enabled,
    id.is_tmux_only(),
    id.is_tmux_only()
)
```

And in `validate_config`, append warnings when parsed keybinds target tmux-only actions while `tmux_enabled=false`.

**Step 4: Run test to verify it passes**

Run: `cargo test -p termy_cli providers::tests:: -- --nocapture`  
Run: `cargo test -p termy_cli validate_config::tests:: -- --nocapture`  
Expected: PASS.

**Step 5: Commit**

```bash
git add crates/cli/src/commands/providers.rs crates/cli/src/commands/list_actions.rs crates/cli/src/commands/list_keybinds.rs crates/cli/src/commands/tui.rs crates/cli/src/commands/validate_config.rs
git commit -m "feat(cli): expose command and keybind availability metadata"
```

### Task 6: Make Startup Tmux Failures Actionable (Dialog + Open Config)

**Files:**
- Create: `src/startup.rs`
- Modify: `src/main.rs`
- Modify: `src/terminal_view/mod.rs`
- Test: `src/startup.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn startup_blocker_message_includes_tmux_guidance() {
    let msg = StartupBlocker::TmuxPreflight("tmux 3.3+ required".to_string()).message();
    assert!(msg.contains("tmux_enabled"));
    assert!(msg.contains("restart"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p termy startup_blocker_message_includes_tmux_guidance -- --exact`  
Expected: FAIL with missing `StartupBlocker`.

**Step 3: Write minimal implementation**

```rust
pub enum StartupBlocker {
    TmuxPreflight(String),
    TmuxClientLaunch(String),
    TmuxInitialSnapshot(String),
}

impl StartupBlocker {
    pub fn message(&self) -> String { /* include exact error + recovery text */ }
    pub fn present_and_exit(self) -> ! {
        termy_native_sdk::show_alert("Termy startup blocked", &self.message());
        if termy_native_sdk::confirm("Open config?", "Open config file now?") {
            let _ = crate::app_actions::open_config_file();
        }
        std::process::exit(1);
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p termy startup::tests:: -- --nocapture`  
Expected: PASS.

**Step 5: Commit**

```bash
git add src/startup.rs src/main.rs src/terminal_view/mod.rs
git commit -m "feat(startup): actionable tmux startup failure dialog with config recovery"
```

### Task 7: Make Snapshot Parsing Delimiter-Safe

**Files:**
- Modify: `crates/terminal_ui/src/tmux.rs`
- Test: `crates/terminal_ui/src/tmux.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn parse_snapshot_accepts_escaped_field_delimiters_in_window_name_and_command() {
    let windows = "@1\x1f0\x1fname\\x09with-tab\x1flayout\x1f1\x1f1\n";
    let panes = "%1\x1f@1\x1f$1\x1f1\x1f0\x1f0\x1f80\x1f24\x1f0\x1f0\x1f/tmp\x1fcmd\\x0awith-nl\n";
    let snapshot = parse_snapshot("termy", windows, panes).expect("snapshot");
    assert_eq!(snapshot.windows[0].name, "name\twith-tab");
    assert_eq!(snapshot.windows[0].panes[0].current_command, "cmd\nwith-nl");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p termy_terminal_ui parse_snapshot_accepts_escaped_field_delimiters_in_window_name_and_command -- --exact`  
Expected: FAIL with invalid line parsing.

**Step 3: Write minimal implementation**

```rust
const FIELD_SEP: char = '\u{1f}';

fn decode_snapshot_field(value: &str) -> Result<String> {
    // decode \xHH escapes for delimiters and control chars
}

fn parse_fields<const N: usize>(line: &str) -> Result<[String; N]> {
    // split by FIELD_SEP and validate exact field count
}
```

Update both `list-windows`/`list-panes` format strings to emit `FIELD_SEP` and escaped values.

**Step 4: Run test to verify it passes**

Run: `cargo test -p termy_terminal_ui tmux::tests::parse_snapshot_ -- --nocapture`  
Expected: PASS.

**Step 5: Commit**

```bash
git add crates/terminal_ui/src/tmux.rs
git commit -m "fix(tmux): use delimiter-safe snapshot encoding and decoding"
```

### Task 8: Replace FIFO Timeout Matching With Deterministic Control Worker

**Files:**
- Modify: `crates/terminal_ui/src/tmux.rs`
- Test: `crates/terminal_ui/src/tmux.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn control_state_machine_keeps_notifications_out_of_command_output() {
    let mut sm = ControlStateMachine::default();
    sm.on_line(b"%begin 1").unwrap();
    sm.on_line(b"%output %1 hi\\012").unwrap();
    sm.on_line(b"ok").unwrap();
    let done = sm.on_line(b"%end 1").expect("done");
    assert_eq!(done.output.trim(), "ok");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p termy_terminal_ui control_state_machine_keeps_notifications_out_of_command_output -- --exact`  
Expected: FAIL with missing `ControlStateMachine`.

**Step 3: Write minimal implementation**

```rust
struct ControlStateMachine { /* one in-flight command, deterministic transitions */ }

impl ControlStateMachine {
    fn on_line(&mut self, line: &[u8]) -> Result<Option<ControlCommandResult>, TmuxControlError> {
        // explicit handling for %begin/%end/%error/%output notifications
    }
}
```

Replace queue timeout matching with a single read/write worker that processes one request at a time.

**Step 4: Run test to verify it passes**

Run: `cargo test -p termy_terminal_ui tmux::tests::control_ -- --nocapture`  
Expected: PASS.

**Step 5: Commit**

```bash
git add crates/terminal_ui/src/tmux.rs
git commit -m "fix(tmux): deterministic command correlation in control worker"
```

### Task 9: Add Bounded Backpressure For Control Requests And Notifications

**Files:**
- Modify: `crates/terminal_ui/src/tmux.rs`
- Test: `crates/terminal_ui/src/tmux.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn notification_coalescer_collapses_redundant_refresh_events() {
    let mut c = NotificationCoalescer::default();
    c.push(TmuxNotification::NeedsRefresh);
    c.push(TmuxNotification::NeedsRefresh);
    assert_eq!(c.drain().iter().filter(|n| matches!(n, TmuxNotification::NeedsRefresh)).count(), 1);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p termy_terminal_ui notification_coalescer_collapses_redundant_refresh_events -- --exact`  
Expected: FAIL with missing coalescer.

**Step 3: Write minimal implementation**

```rust
const REQUEST_QUEUE_BOUND: usize = 1024;
const NOTIFICATION_QUEUE_BOUND: usize = 2048;

let (request_tx, request_rx) = flume::bounded(REQUEST_QUEUE_BOUND);
let (notifications_tx, notifications_rx) = flume::bounded(NOTIFICATION_QUEUE_BOUND);
```

Add coalescing rules for repetitive `NeedsRefresh` and burst pane output notifications.

**Step 4: Run test to verify it passes**

Run: `cargo test -p termy_terminal_ui tmux::tests::notification_ -- --nocapture`  
Expected: PASS.

**Step 5: Commit**

```bash
git add crates/terminal_ui/src/tmux.rs
git commit -m "perf(tmux): add bounded queues and refresh coalescing"
```

### Task 10: Add High-Volume Input Path For Tmux Send Input

**Files:**
- Modify: `crates/terminal_ui/src/tmux.rs`
- Modify: `src/terminal_view/interaction/input.rs`
- Test: `crates/terminal_ui/src/tmux.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn send_input_uses_bulk_path_for_large_payloads() {
    let (mode, chunks) = choose_send_input_mode(8192);
    assert_eq!(mode, SendInputMode::Bulk);
    assert!(chunks < 64);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p termy_terminal_ui send_input_uses_bulk_path_for_large_payloads -- --exact`  
Expected: FAIL with missing bulk mode helper.

**Step 3: Write minimal implementation**

```rust
enum SendInputMode { ChunkedHex, Bulk }

fn choose_send_input_mode(bytes_len: usize) -> (SendInputMode, usize) {
    if bytes_len >= 2048 { (SendInputMode::Bulk, 1) } else { (SendInputMode::ChunkedHex, bytes_len.div_ceil(256)) }
}
```

Implement bulk path with explicit flow-control acknowledgment before returning to caller.

**Step 4: Run test to verify it passes**

Run: `cargo test -p termy_terminal_ui tmux::tests::send_input_ -- --nocapture`  
Expected: PASS.

**Step 5: Commit**

```bash
git add crates/terminal_ui/src/tmux.rs src/terminal_view/interaction/input.rs
git commit -m "perf(tmux): add high-volume input path with explicit flow control"
```

### Task 11: Remove 16ms Polling And Switch To Event-Driven Tmux Wakeups

**Files:**
- Modify: `crates/terminal_ui/src/tmux.rs`
- Modify: `src/terminal_view/mod.rs`
- Test: `src/terminal_view/mod.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn tmux_runtime_uses_event_driven_wakeup_strategy() {
    assert!(TerminalView::uses_event_driven_tmux_wakeup());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p termy tmux_runtime_uses_event_driven_wakeup_strategy -- --exact`  
Expected: FAIL because polling strategy constant and branch still exist.

**Step 3: Write minimal implementation**

```rust
// remove TMUX_POLL_INTERVAL_MS and polling spawn branch
// send wakeup signal from tmux notification producer
while event_wakeup_rx.recv_async().await.is_ok() {
    while event_wakeup_rx.try_recv().is_ok() {}
    // drain both native + tmux events
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p termy terminal_view::tests:: -- --nocapture`  
Expected: PASS.

**Step 5: Commit**

```bash
git add crates/terminal_ui/src/tmux.rs src/terminal_view/mod.rs
git commit -m "perf(ui): replace tmux polling loop with event-driven wakeups"
```

### Task 12: Remove Blocking Resize/Snapshot Work From UI Thread

**Files:**
- Create: `src/terminal_view/tmux_sync.rs`
- Modify: `src/terminal_view/mod.rs`
- Modify: `src/terminal_view/interaction/layout.rs`
- Test: `src/terminal_view/tmux_sync.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn resize_scheduler_coalesces_multiple_resize_requests() {
    let mut scheduler = TmuxResizeScheduler::default();
    scheduler.request_resize(120, 40);
    scheduler.request_resize(121, 40);
    assert_eq!(scheduler.take_pending(), Some((121, 40)));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p termy resize_scheduler_coalesces_multiple_resize_requests -- --exact`  
Expected: FAIL with missing `TmuxResizeScheduler`.

**Step 3: Write minimal implementation**

```rust
// in tmux_sync.rs
pub struct TmuxResizeScheduler { pending: Option<(u16, u16)> }
// schedule async refresh; never call std::thread::sleep on UI thread
```

Replace `refresh_tmux_snapshot_for_client_size` blocking retry loop with async convergence jobs.

**Step 4: Run test to verify it passes**

Run: `cargo test -p termy tmux_sync::tests:: -- --nocapture`  
Expected: PASS.

**Step 5: Commit**

```bash
git add src/terminal_view/tmux_sync.rs src/terminal_view/mod.rs src/terminal_view/interaction/layout.rs
git commit -m "perf(tmux): move resize and snapshot convergence off UI thread"
```

### Task 13: Introduce Runtime Backend Boundary For `TerminalView`

**Files:**
- Create: `src/terminal_view/backend.rs`
- Modify: `src/terminal_view/mod.rs`
- Modify: `src/terminal_view/tabs/lifecycle.rs`
- Modify: `src/terminal_view/interaction/input.rs`
- Modify: `src/terminal_view/interaction/layout.rs`
- Test: `src/terminal_view/backend.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn backend_mode_reports_tmux_without_leaking_tmux_client_type() {
    let mode = RuntimeBackendMode::Tmux;
    assert!(mode.uses_tmux());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p termy backend_mode_reports_tmux_without_leaking_tmux_client_type -- --exact`  
Expected: FAIL with missing backend boundary types.

**Step 3: Write minimal implementation**

```rust
pub trait RuntimeBackend {
    fn mode(&self) -> RuntimeBackendMode;
    fn process_events(&mut self) -> BackendEventBatch;
    fn resize(&mut self, cols: u16, rows: u16);
    fn request_snapshot_refresh(&mut self);
}
```

Move direct `TmuxClient` orchestration out of `TerminalView` into backend implementation.

**Step 4: Run test to verify it passes**

Run: `cargo test -p termy backend::tests:: -- --nocapture`  
Expected: PASS.

**Step 5: Commit**

```bash
git add src/terminal_view/backend.rs src/terminal_view/mod.rs src/terminal_view/tabs/lifecycle.rs src/terminal_view/interaction/input.rs src/terminal_view/interaction/layout.rs
git commit -m "refactor(terminal_view): introduce runtime backend boundary for tmux add-on mode"
```

### Task 14: Remove Duplicated Native/Tmux Read Paths

**Files:**
- Modify: `src/terminal_view/mod.rs`
- Modify: `src/terminal_view/search.rs`
- Modify: `src/terminal_view/interaction/selection.rs`
- Modify: `src/terminal_view/render.rs`
- Modify: `src/terminal_view/titles/source.rs`
- Test: `src/terminal_view/search.rs`
- Test: `src/terminal_view/interaction/selection.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn terminal_read_adapter_extracts_lines_for_both_runtime_variants() {
    // construct minimal test terminal and assert unified accessor returns line text
    assert!(true); // replace with concrete adapter assertion
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p termy terminal_read_adapter_extracts_lines_for_both_runtime_variants -- --exact`  
Expected: FAIL (adapter missing).

**Step 3: Write minimal implementation**

```rust
impl Terminal {
    fn with_renderable_content<R>(&self, f: impl FnOnce(RenderableView<'_>) -> R) -> Option<R> {
        // one lock/match location only
    }
}
```

Replace repeated `match Terminal::Tmux|Native` blocks in search/selection/render/title paths.

**Step 4: Run test to verify it passes**

Run: `cargo test -p termy search::tests:: -- --nocapture`  
Run: `cargo test -p termy interaction::selection::tests:: -- --nocapture`  
Expected: PASS.

**Step 5: Commit**

```bash
git add src/terminal_view/mod.rs src/terminal_view/search.rs src/terminal_view/interaction/selection.rs src/terminal_view/render.rs src/terminal_view/titles/source.rs
git commit -m "refactor(terminal): dedupe native/tmux read paths via unified terminal accessor"
```

### Task 15: Generate Tmux Gating Notes In Docs And Templates

**Files:**
- Modify: `crates/xtask/src/main.rs`
- Modify: `docs/keybindings.md` (generated)
- Modify: `docs/configuration.md` (generated)
- Modify: `crates/config_core/src/default_config.txt` (generated, if template text changes)
- Test: `crates/xtask/src/main.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn keybindings_doc_marks_tmux_only_actions() {
    let out = render_keybindings_doc();
    assert!(out.contains("tmux required"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p xtask keybindings_doc_marks_tmux_only_actions -- --exact`  
Expected: FAIL; docs currently do not annotate tmux-only defaults/actions.

**Step 3: Write minimal implementation**

```rust
let tmux_note = if binding.action.is_tmux_only() { " (tmux required; restart required)" } else { "" };
output.push_str(&format!("- `{}` -> `{}`{}\n", binding.trigger, binding.action.config_name(), tmux_note));
```

Add one canonical tmux note in configuration docs (supported platforms + restart semantics).

**Step 4: Run test to verify it passes**

Run: `cargo test -p xtask -- --nocapture`  
Run: `cargo run -p xtask -- generate-keybindings-doc`  
Run: `cargo run -p xtask -- generate-config-doc`  
Expected: PASS and regenerated docs.

**Step 5: Commit**

```bash
git add crates/xtask/src/main.rs docs/keybindings.md docs/configuration.md crates/config_core/src/default_config.txt
git commit -m "docs: generate explicit tmux gating and restart semantics"
```

### Task 16: Final Verification And Architecture Boundary Checks

**Files:**
- Modify: `scripts/check-boundaries.sh` (only if boundary policy intentionally changes)
- Modify: `.github/workflows/architecture-checks.yml` (only if checks must change)

**Step 1: Write failing checks intentionally avoided**

No code changes in this task. Use this task to run full verification and capture any regressions introduced by previous tasks.

**Step 2: Run verification suite**

Run: `cargo check -p termy_command_core`  
Run: `cargo check -p termy_terminal_ui`  
Run: `cargo check -p termy_cli`  
Run: `cargo check -p termy`  
Run: `cargo test -p termy_command_core`  
Run: `cargo test -p termy_terminal_ui`  
Run: `cargo test -p termy_cli`  
Run: `cargo test -p termy`  
Run: `cargo run -p xtask -- generate-keybindings-doc --check`  
Run: `cargo run -p xtask -- generate-config-doc --check`  
Run: `bash scripts/check-boundaries.sh`

Expected: PASS for all commands.

**Step 3: Minimal fixes for breakages**

Address only root-cause failures revealed by the suite. No fallback behavior, no silent defaults.

**Step 4: Re-run verification**

Run the same command list and require all PASS.

**Step 5: Commit**

```bash
git add -A
git commit -m "chore: verify tmux hardening rollout and boundary checks"
```

---

## Rollout Notes

1. Keep each task as an independent commit for clean bisect.
2. Do not run `cargo fmt` unless explicitly requested.
3. Prefer removing legacy branches over adding compatibility switches.
4. If an intermediate architecture change blocks progress, land a no-behavior-change refactor commit first, then continue with behavior changes.

## Suggested Execution Order

1. Tasks 1-5 (availability and UX consistency).
2. Tasks 6-12 (startup + tmux runtime correctness/performance).
3. Tasks 13-14 (backend boundary and dedup cleanup).
4. Tasks 15-16 (docs generation and full verification).


# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and Run Commands

```bash
# Run the app
cargo run -p termy

# Run release build
cargo run --release

# Check workspace
cargo check --workspace

# Run tests
cargo test --release

# Run specific crate tests
cargo test -p termy_config_core

# Tmux integration tests (requires tmux >= 3.3)
just test-tmux-integration

# Verify crate dependency boundaries
just check-boundaries

# Regenerate docs after changing config/keybinds
just generate-keybindings-doc
just generate-config-doc

# Development with auto-reload (requires cargo-watch)
just dev
```

## Architecture

Termy is a terminal emulator built with **GPUI** (Zed's UI framework) and **alacritty_terminal** for terminal emulation.

### Source Structure

- `src/` - Main GPUI desktop application
  - `terminal_view/` - Terminal rendering, interaction, tabs, search, pane splitting
  - `settings_view/` - Settings UI
  - `commands.rs` - Command dispatch and keybind handling
  - `config/` - Config loading and app-level adapter
- `crates/` - Workspace crates with strict dependency boundaries

### Key Crates and Boundaries

These dependency rules are enforced by `just check-boundaries`:

| Crate | Purpose | Must NOT depend on |
|-------|---------|-------------------|
| `termy_command_core` | Command IDs, keybind defaults, resolution | `gpui`, `termy_config_core` |
| `termy_config_core` | Config parsing, schema | `termy_themes` |
| `termy_cli` | CLI tool | `gpui` |
| `termy_cli_install_core` | CLI installation logic | `gpui` |

Other notable crates:
- `termy_native_sdk` - Native OS integrations (context menus, window controls)
- `termy_terminal_ui` - Terminal rendering primitives
- `termy_themes` - Theme definitions and parsing
- `xtask` - Build tasks, doc generation, benchmarking

### Platform-Specific Code

Use conditional compilation for platform differences:
```rust
#[cfg(target_os = "macos")]
fn macos_specific() { ... }

#[cfg(not(target_os = "macos"))]
fn non_macos_fallback() { ... }

#[cfg(target_os = "windows")]
fn windows_specific() { ... }
```

### Generated Files

Do NOT edit these manually - regenerate with `just generate-*`:
- `docs/keybindings.md`
- `docs/configuration.md`

### Rendering Performance

Terminal rendering uses cell caching with dirty-span tracking. Debug render metrics:
```bash
RUST_LOG=info TERMY_RENDER_METRICS=1 cargo run -p termy
```
- `full`: full cell cache rebuilds (should stay near 0 during cursor blink)
- `partial`: dirty-span patch updates
- `reuse`: no cache update needed

## Plugin System

Plugins are out-of-process executables using stdio JSON protocol. Located in `<config-dir>/plugins/<plugin-id>/`. See `docs/architecture/plugins.md` for the protocol spec.

## Clippy Configuration

Custom thresholds in `clippy.toml`:
- `too-many-arguments-threshold = 8` (GPUI callbacks need many params)
- `cognitive-complexity-threshold = 30` (rendering functions are complex)

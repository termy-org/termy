# Testing strategy

Where tests live and which command to run for a given change.

## Pyramid

| Layer | What | Where | Command |
|-------|------|-------|---------|
| **Unit** | Pure logic, parsers, catalogs | `config_core`, `command_core`, `search`, `core`, inline `#[test]` | `cargo test -p <crate>` |
| **Integration** | Tmux client, grid, FFI | `terminal_ui/tests/`, `ffi` | `cargo test -p termy_terminal_ui` |
| **App** | GPUI terminal view, settings, commands | `desktop_app` | `just test` |
| **Platform** | macOS Swift config matrix | `macos/` | `just test-macos-config` |
| **Manual** | Visual chrome, GPU paint | — | Run app; see [development.md](../development.md) render metrics |

## Ignored tests

- `crates/terminal_ui/tests/tmux_split_integration.rs` — requires **tmux ≥ 3.3** locally.
- Run: `just test-tmux-integration`
- CI: macOS `architecture-checks` job (when tmux available).

Every `#[ignore]` must reference a tracking issue in a comment.

## Before opening a PR

Use the **smallest** pass that proves your change:

```sh
cargo check -p termy              # UI-only tweak
cargo test -p termy_config_core   # config schema/parser
just check-boundaries             # deps, generated docs, commands
just test-workspace               # broad Rust change (target: matches CI E0.1)
just validate                     # full local gate (once E0 lands)
```

## Roadmap

CI parity and tmux reliability: [roadmap.md](roadmap.md) phase E0–E2.

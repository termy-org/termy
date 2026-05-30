# Project Layout

Termy is a single repository with several product surfaces. Keep changes in the smallest owner that can express the behavior.

## Main Areas

- `crates/desktop_app/` owns the desktop app shell: GPUI windows, app chrome, settings, onboarding, menus, and app-only interaction behavior.
- `crates/desktop_app/src/terminal_view/` owns the GPUI terminal experience: rendering, tabs, panes, command palette, search UI, mouse/input handling, and app runtime coordination.
- `crates/core/` owns the reusable headless libtermy runtime/API used by embedders. It must stay independent of GPUI and app UI code.
- `crates/terminal_ui/` owns the GPUI-facing terminal grid/runtime adapter, native pane model, and tmux support used by the desktop app.
- `crates/config_core/`, `crates/command_core/`, `crates/theme_core/`, and `crates/search/` own pure domain logic shared by the app, CLI, docs generation, and embedding surfaces.
- `crates/ffi/` exposes libtermy to C-compatible hosts.
- `website/` owns the public website and user-facing docs.
- `docs/` owns contributor and architecture docs inside the repo.
- `scripts/` owns packaging, platform, and release support.
- `assets/` owns app icons, shell completions, UI icons, and media.

## Workspace Crates

- `termy` (`crates/desktop_app/`) is the product app and the only crate that should own complete user-facing desktop workflows.
- `termy_core` (`crates/core/`) is the headless runtime/API for embedders.
- `gpui-native-appkit` (`crates/gpui_native_appkit/`) is the reusable AppKit/SwiftUI titlebar bridge for GPUI-hosted macOS windows.
- `termy_terminal_ui` (`crates/terminal_ui/`) is the GPUI-facing terminal adapter used by the desktop app.
- `termy_command_core`, `termy_config_core`, `termy_theme_core`, `termy_search`, and `termy_themes` are pure domain crates.
- `termy_ffi` and `termy_native_sdk` are embedding/native-integration surfaces.
- `termy_cli`, `termy_cli_install_core`, `termy_release_core`, `termy_auto_update`, and `termy_auto_update_ui` own command-line, install, release, and update support.
- `termy_toast` owns tiny notification primitives.
- `xtask` owns repository automation such as generated docs.

Each crate should have a local `README.md` that explains its owner boundary, allowed dependencies, and common validation command.
The `crates/README.md` file is the workspace crate index.

## Boundary Rules

- Keep `termy_core` headless. Do not add GPUI, app chrome, command palette, or desktop-window concerns there.
- Keep pure domain crates free of GPUI and app-specific presentation code.
- Keep `termy_ffi` aligned with the reusable core contract rather than copying desktop app behavior.
- Put user-visible app behavior in `crates/desktop_app/`; extract to sibling crates only when the behavior is shared by multiple surfaces or needs isolated tests.
- Update repo docs and public website docs together when a public behavior or embedding contract changes.
- Keep release packaging rooted in `scripts/` and documented in [Release Packaging](release-packaging.md). Do not add parallel packaging entrypoints unless they become the documented source of truth.
- Keep root-level directory indexes (`crates/README.md`, `scripts/README.md`) aligned with ownership changes.

## Validation

Use `just check-boundaries` after changing crate dependencies, generated docs, command/keybind behavior, or config behavior. Use `cargo check --workspace` after moving modules or changing cross-crate contracts.

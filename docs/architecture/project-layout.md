# Project Layout

Termy is a single repository with several product surfaces. Keep changes in the smallest owner that can express the behavior.

## Main Areas

- `crates/desktop_app/` owns the desktop app shell: GPUI windows, app chrome, settings, onboarding, menus, and app-only interaction behavior.
- `crates/desktop_app/src/terminal_view/` owns the GPUI terminal experience: rendering, tabs, panes, command palette, search UI, mouse/input handling, and app runtime coordination.
- `crates/core/` owns the reusable headless libtermy runtime/API used by embedders. It must stay independent of GPUI and app UI code.
- `crates/terminal_ui/` owns the GPUI-facing terminal grid/runtime adapter, native pane model, and tmux support used by the desktop app.
- `crates/config_core/`, `crates/command_core/`, `crates/theme_core/`, and `crates/search/` own pure domain logic shared by the app, CLI, docs generation, and embedding surfaces.
- `crates/ffi/` and `crates/wasm/` expose libtermy to C-compatible hosts and WebAssembly consumers.
- `packages/libtermy-js/` owns the JavaScript package around the WASM build and browser renderer helpers.
- `examples/` contains consumer-facing examples for Rust, C, Swift, and browser integrations.
- `website/` owns the public website and user-facing docs.
- `docs/` owns contributor and architecture docs inside the repo.
- `scripts/`, `installer/`, and `macos/` own packaging, platform, and release support.

## Boundary Rules

- Keep `termy_core` headless. Do not add GPUI, app chrome, command palette, or desktop-window concerns there.
- Keep pure domain crates free of GPUI and app-specific presentation code.
- Keep `termy_ffi`, `termy_wasm`, and `libtermy.js` aligned with the reusable core contract rather than copying desktop app behavior.
- Put user-visible app behavior in `crates/desktop_app/`; extract to sibling crates only when the behavior is shared by multiple surfaces or needs isolated tests.
- Update repo docs and public website docs together when a public behavior or embedding contract changes.

## Validation

Use `just check-boundaries` after changing crate dependencies, generated docs, command/keybind behavior, or config behavior. Use `cargo check --workspace` after moving modules or changing cross-crate contracts.

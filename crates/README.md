# Crates

Termy is a Rust workspace split by ownership boundary, not by implementation convenience. Prefer the smallest crate that owns the behavior.

## Product Surface

- `desktop_app/` (`termy`): GPUI desktop app, windows, app chrome, settings, onboarding, command execution, and user-visible desktop workflows.
- `cli/` (`termy_cli`): `termy-cli` command-line companion.
- `ffi/` (`termy_ffi`): C-compatible libtermy surface.

## Runtime And UI

- `core/` (`termy_core`): headless terminal runtime/API for embedders.
- `terminal_ui/` (`termy_terminal_ui`): GPUI-facing terminal adapter, grid paint cache, native pane runtime, and tmux support.
- `native_sdk/` (`termy_native_sdk`): narrow platform-native helpers.

## Pure Domain Crates

- `command_core/` (`termy_command_core`): command IDs, keybinding-facing definitions, and command metadata.
- `config_core/` (`termy_config_core`): config schema, defaults, and config-facing types.
- `theme_core/` (`termy_theme_core`): theme data model.
- `themes/` (`termy_themes`): bundled theme definitions.
- `search/` (`termy_search`): reusable terminal search primitives.

## Release, Install, And Support

- `release_core/` (`termy_release_core`): release metadata and version helpers.
- `auto_update/` (`termy_auto_update`): update discovery, verification, and platform update decisions.
- `auto_update_ui/` (`termy_auto_update_ui`): reusable update UI wrappers.
- `cli_install_core/` (`termy_cli_install_core`): shared CLI install/path helpers.
- `toast_sdk/` (`termy_toast`): small notification primitives.
- `xtask/` (`xtask`): repository automation and generated-doc checks.

Each crate has its own `README.md`. Update the local README when a crate gains or loses ownership of a responsibility.

## Dependency Rules

- `termy_core`, `termy_ffi`, `termy_cli`, `termy_cli_install_core`, and pure domain crates must not depend on GPUI.
- `termy_ffi` should wrap `termy_core`, not copy desktop app behavior.
- `termy_command_core` must stay independent of config parsing and UI presentation.
- App-only behavior belongs in `desktop_app/` until another product surface needs it.

Run `just check-boundaries` after changing crate dependencies, generated docs, command/keybind behavior, or config behavior.

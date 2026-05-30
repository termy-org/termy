# termy

Main desktop application.

This crate owns the GPUI app shell, windows, titlebar/chrome, menus, settings, onboarding, command execution, and user-visible desktop workflows.

Important internal areas:

- `src/terminal_view/`: terminal surface, tabs, panes, search, command palette, input, rendering, persistence, and runtime coordination.
- `src/settings_view/`: settings UI and state application.
- `src/onboarding/`: first-run and import flows.
- `src/config/`: app-owned config I/O and mutation.

Push reusable headless behavior into `termy_core` or a pure domain crate. Push GPUI-adjacent terminal adapter behavior into `termy_terminal_ui` only when it is reusable outside the desktop app shell.

Validation:

```sh
cargo test -p termy
cargo check -p termy
```

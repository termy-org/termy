# termy_terminal_ui

GPUI-facing terminal runtime support for the desktop app.

This crate owns the terminal grid paint cache, native pane runtime, tmux runtime/client support, and GPUI-adjacent terminal adapter behavior used by `src/terminal_view/`. It can depend on GPUI, but shared headless behavior should live in `termy_core` or a pure domain crate instead.

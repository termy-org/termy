# gpui-native-appkit

Native AppKit window-tab helpers for GPUI-hosted macOS windows.

This crate owns the bridge between a `gpui::Window` and AppKit `NSWindow` tabbing. It exists so the desktop app can opt into real system-managed macOS tabs even though GPUI owns the window host.

## Capability

- Resolve GPUI's macOS `NSView` through `raw-window-handle`.
- Configure the owning `NSWindow` for AppKit tabbing.
- Add a newly opened GPUI window to another window's `NSWindowTabGroup`.
- Return `UnsupportedPlatform` on non-macOS targets.

## Usage

```rust
use gpui_native_appkit::{add_window_to_tab_group, configure_window_tabbing};

configure_window_tabbing(current_window, "Shell")?;
configure_window_tabbing(new_window, "Logs")?;
add_window_to_tab_group(current_window, new_window)?;
```

## Boundary

This crate should stay focused on AppKit interop for GPUI-hosted windows. Product-specific tab state, pane behavior, keyboard shortcuts, and rendering policy belong in `crates/desktop_app/`.

Validation:

```sh
cargo test -p gpui-native-appkit
cargo check -p gpui-native-appkit
```

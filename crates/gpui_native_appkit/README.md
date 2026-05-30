# gpui-native-appkit

Native AppKit and SwiftUI titlebar helpers for GPUI-hosted macOS windows.

This crate owns the bridge between a `gpui::Window` and AppKit titlebar accessory views. It exists so the desktop app can use native SwiftUI controls in the titlebar even though GPUI owns the window host.

## Capability

- Resolve GPUI's macOS `NSView` through `raw-window-handle`.
- Attach an `NSTitlebarAccessoryViewController` to the owning `NSWindow`.
- Render SwiftUI titlebar tabs through `NSHostingView`.
- Keep tab selection and new-tab actions flowing back to Rust through a narrow C callback.
- Return `UnsupportedPlatform` on non-macOS targets.

## Usage

```rust
use gpui_native_appkit::{
    NativeTitlebarTab, NativeTitlebarTabsOptions, install_or_update_titlebar_tabs,
};

let tabs = [
    NativeTitlebarTab::new("tab-1", "Shell").selected(true),
    NativeTitlebarTab::new("tab-2", "Logs"),
];

install_or_update_titlebar_tabs(
    window,
    NativeTitlebarTabsOptions::new(&tabs).selected_id("tab-1"),
)?;
```

## Boundary

This crate should stay focused on AppKit/SwiftUI interop for GPUI-hosted windows. Product-specific tab state, pane behavior, keyboard shortcuts, and rendering policy belong in `crates/desktop_app/`.

Validation:

```sh
cargo test -p gpui-native-appkit
cargo check -p gpui-native-appkit
```


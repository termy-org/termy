# termy_native_sdk

Narrow native OS integration layer.

This crate owns reusable platform-specific primitives that are cleaner outside the desktop app crate, such as macOS AppKit/Foundation helpers or Windows shell integration helpers.

Keep product workflows in `crates/desktop_app/`; keep cross-platform terminal behavior in `termy_core`.

Validation:

```sh
cargo test -p termy_native_sdk
```

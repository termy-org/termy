# libtermy Swift Example

This SwiftPM example imports `crates/ffi/include/termy.h` through a small Clang
module, links against the local debug `termy_ffi` dynamic library, and renders a
live libtermy terminal snapshot in SwiftUI. It calls `termy_config_load_default`
before terminal creation, so it uses the normal Termy config file when one
exists and falls back to defaults otherwise.

Run it from the repo root:

```sh
cargo build -p termy_ffi
swift run --package-path examples/libtermy-swift
```

For terminal-only smoke tests, run:

```sh
TERMY_SWIFT_EXAMPLE_EXIT_AFTER_RENDER=1 swift run --package-path examples/libtermy-swift
```

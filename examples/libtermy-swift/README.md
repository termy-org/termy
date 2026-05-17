# libtermy Swift Example

This SwiftPM example imports `crates/ffi/include/termy.h` through a small Clang
module, links against the local debug `termy_ffi` dynamic library, and renders a
live libtermy terminal snapshot in SwiftUI. It calls `termy_config_load_default`
before terminal creation, so it uses the normal Termy config file when one
exists and falls back to defaults otherwise. The loaded config is also queried
for renderer settings such as font size, line height, padding, and background
opacity before the SwiftUI terminal surface is laid out. Theme colors from the
same config are applied to the terminal snapshots and SwiftUI surface.

The example also demonstrates the embedder-facing event and search APIs:
`drainEvents()` returns structured runtime events such as title, working
directory, progress, and exit updates; `search(_:)` returns visible-frame
matches with row/column positions and line text. The app uses SwiftUI `TabView`
for native tabs and renders a small top loader from libtermy progress events.

Run it from the repo root:

```sh
cargo build -p termy_ffi
swift run --package-path examples/libtermy-swift
```

Or build and open a dev `.app` bundle:

```sh
./examples/libtermy-swift/run.sh
```

For terminal-only smoke tests, run:

```sh
TERMY_SWIFT_EXAMPLE_EXIT_AFTER_RENDER=1 swift run --package-path examples/libtermy-swift
```

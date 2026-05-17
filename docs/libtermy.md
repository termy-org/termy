# libtermy

`libtermy` is the embeddable Termy terminal engine. The first cut is split into:

- `termy_core`: Rust API for a single headless terminal surface.
- `termy_ffi`: C ABI wrapper over `termy_core`.

The core API owns PTY startup, terminal parsing, input writes, resize, event
draining, damage snapshots, and renderer-neutral frame snapshots. It does not
depend on GPUI and does not expose Termy's app chrome, tabs, panes, or tmux
session model as part of the public v1 surface.

## Rust Embedding

Use `termy_core::Terminal` directly:

```rust
let terminal = termy_core::Terminal::new(
    termy_core::TerminalSize::default(),
    None,
    None,
    None,
    Some(&termy_core::TerminalRuntimeConfig::default()),
    None,
)?;
terminal.write(b"echo hello\r");
let frame = terminal.snapshot();
```

`TermyFrame` contains a flat row-major `Vec<TermyCell>`, cursor state, scroll
state, and cell colors as simple RGBA bytes.

See `examples/libtermy-rust/` for a minimal Rust embedding.

## C ABI

Use `termy_ffi` as an opaque-handle API:

- `crates/ffi/include/termy.h`
- `termy_terminal_new`
- `termy_terminal_free`
- `termy_terminal_write`
- `termy_terminal_resize`
- `termy_terminal_snapshot`
- `termy_frame_free`
- `termy_terminal_take_damage`
- `termy_damage_free`
- `termy_terminal_drain_events`
- `termy_event_batch_free`

Any returned frame, damage, event batch, or standalone byte payload must be
released by the matching `termy_*_free` function. Event payloads owned by an
event batch are freed by `termy_event_batch_free`; do not free them separately.
Embedders should synchronize access to a terminal handle if they call into it
from multiple threads.

Event kind values:

- `1`: wakeup
- `2`: title
- `3`: reset title
- `4`: bell
- `5`: exit
- `6`: clipboard store
- `7`: shell prompt start
- `8`: shell command start
- `9`: shell command executing
- `10`: shell command finished
- `11`: progress
- `12`: working directory

Cursor style values:

- `1`: line
- `2`: block

See `examples/libtermy-c/` and `examples/libtermy-swift/` for C and Swift
embedding examples.

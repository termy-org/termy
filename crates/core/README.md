# termy_core

Reusable headless libtermy runtime and API.

This crate owns terminal lifecycle, frame snapshots, keyboard/mouse protocol helpers, search over frames, config-to-runtime conversion, shell integration state, and embedder-facing render metrics. It must remain independent of GPUI and desktop app chrome.

Use this crate when behavior should be available to FFI, WASM, JS, or non-GPUI host examples.

## Why libtermy instead of `alacritty_terminal` directly?

`alacritty_terminal` is a VT parser, grid, and PTY driver. It stops at terminal emulation. libtermy wraps it with the harness every embedder ends up writing anyway:

- **Stable frame API** — `TermyFrame` / `TermyCell` / `TermyColor` snapshots so renderers do not couple to alacritty's internal grid types.
- **Damage tracking** — `TerminalDamageSnapshot` and dirty spans for partial redraws and cell-cache renderers.
- **Input encoding** — `keystroke_to_input`, mouse report encoder, and keyboard/mouse mode state (xterm, SGR, kitty, etc.).
- **Search** — `search_frame` over snapshots without re-implementing grid traversal.
- **Shell integration** — OSC 133 / 633 / 1337 command lifecycle, progress state, and tab title resolution.
- **OSC interception** — clipboard, color queries, and a reply-host abstraction so embedders do not parse OSC themselves.
- **Link detection** — URL and path classification for click-to-open.
- **Config bridge** — `AppConfig` → `TerminalRuntimeConfig`, theme color resolution, terminal query colors.
- **Font + cell metrics** — `measure_cell` backed by fontdb / ttf-parser for embedder layout.
- **Render metrics** — span timings (grid paint, text shaping, cache hit/miss) for performance debugging.
- **Launch resolution** — working directory normalization, fallbacks, locale, and PATH setup.
- **PTY plumbing** — `rustix-openpty` on Unix, shell discovery via `winreg` on Windows.
- **Portable surface** — no GPUI dependency. The same crate powers the desktop app, `termy_ffi` (C ABI), WASM hosts, JS bindings, and headless tests. Wrap alacritty once, host many times.

In short: `alacritty_terminal` is the engine; libtermy is the engine plus the harness embedders actually need.

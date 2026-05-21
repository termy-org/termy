# termy_core

Reusable headless libtermy runtime and API.

This crate owns terminal lifecycle, frame snapshots, keyboard/mouse protocol helpers, search over frames, config-to-runtime conversion, shell integration state, and embedder-facing render metrics. It must remain independent of GPUI and desktop app chrome.

Use this crate when behavior should be available to FFI, WASM, JS, or non-GPUI host examples.

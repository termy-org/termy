# TermyAlpha

Higly experimental not ready for daily use. 

Native macOS 14+ SwiftUI terminal host backed by the repo-local `libtermy`.

## Run

```sh
./script/build_and_run.sh
```

The script builds `crates/ffi` from the repository root first, then builds and launches the SwiftPM app as `macos/dist/TermyAlpha.app`.

At startup the app reads Termy's local config, including `working_dir`, `window_width`, and `window_height`.

## Shortcuts

- `Cmd+T`: new native macOS window tab
- `Cmd+D`: split right
- `Cmd+Shift+D`: split down
- `Cmd+Shift+W`: close focused pane
- `Cmd+]`: focus next pane
- `Cmd+F`: search the focused pane
- `Ctrl+C`: send interrupt to the focused terminal
- `Shift+Tab`: backtab through Termy's keyboard encoder
- Mouse or trackpad scroll: move through the focused pane's scrollback
- The right-side scrollbar appears while scrolling; drag its thumb to move through scrollback
- Drag split dividers with the mouse to resize panes

Keyboard input is encoded through repo-local `termy_core`, including Kitty keyboard protocol modes when terminal applications negotiate them.

## Validate

```sh
./scripts/check-config-matrix.sh
./scripts/stress-native.sh
./scripts/check-release-readiness.sh
```

`check-config-matrix.sh` runs Swift regression tests for shared config/schema parity. `stress-native.sh` runs persistence, selection, and render-clamping stress tests; pass `--launch` for a local app launch smoke. `check-release-readiness.sh` verifies native bundle metadata plus signing/notarization hooks, and accepts `--app PATH` to inspect a staged `.app`.

For a staged app bundle, run a local startup/RSS/CPU gate with:

```sh
./scripts/check-launch-gate.sh --app .build/dmg-arm64/TermyAlpha.app
```

Native DMGs are built with `./scripts/build-dmg.sh`. Pass `--sign-identity` plus notary credentials to sign/notarize, or use `./scripts/build-dmg-signed.sh` when a missing signing identity should fail loudly.

Performance benchmark summaries from `cargo run -p xtask -- benchmark-compare` can be gated with:

```sh
./scripts/check-performance-gates.sh --summary target/macos-performance-gate/summary.json
```

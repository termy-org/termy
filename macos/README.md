# TermyAlpha

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

# libtermy-demo

Standalone browser demo that exercises every public surface of
[`libtermy.js`](../../packages/libtermy-js)'s `createTermyRenderer` API
end-to-end against an in-browser fake PTY.

## Features demonstrated

Top-bar controls:

- **Mode toggle** — `main-thread` (default `workerized: false`) vs `workerized`
  (`workerized: true`). Switching tears the renderer down and rebuilds it.
- **Backend selector** — `auto` / `canvas2d` / `webgl` / `webgpu` / `headless`.
  Headless mode hides the canvas and renders a live JSON dump of
  `term.snapshot()` instead.
- **Size selector** — `80 x 24` / `100 x 32` / `132 x 40` / `200 x 60`, calling
  `term.resize(cols, rows)` in place.
- **Rebuild renderer** — tears down + reconstructs with the current settings.

Startup script (replayable from the sidebar) writes:

- 16-color test pattern (foreground + background)
- 256-color cube + grayscale ramp
- 24-bit true-color RGB gradient + HSL rainbow band
- Bold / dim / italic / underline / reverse attributes
- **CJK + wide cells** — `日本語テスト こんにちは 世界 🌏 中文测试 한국어 시험`
- **Emoji** — `🦀🎉🚀✨💀🔥🌈🐢🦄`, plus combining accents
- **OSC8 hyperlink** (`ESC ] 8 ; ; URI ESC \ label ESC ] 8 ; ; ESC \`) on one
  line, plus a regex-detected `https://...` URL on a sibling line so you can
  compare the two paths
- **Mouse protocol** — toggles SGR button-event + button-press + any-event
  mouse modes (`CSI ?1000;1006h`, `CSI ?1002;1006h`, `CSI ?1003;1006h`) at the
  end of the script so hovering / clicking inside the terminal echoes
  `CSI < ...` mouse reports through `onInput` (visible in the Log panel)
- 5,050 scrollback lines

Sidebar:

- **Search** — `<input>` plus `Find next` / `Find prev` / `Clear` buttons wired
  to `term.searchAndHighlight()`, `term.findNextMatch()`,
  `term.findPreviousMatch()`, `term.clearSearchHighlight()`. Match count and
  active position are shown live.
- **Bracketed paste** — `Toggle bracketed paste` button writes `CSI ?2004h`
  / `CSI ?2004l`. Current state is shown. When ON, `Cmd+V` pastes into the
  terminal arrive wrapped in `CSI 200~ … CSI 201~` and appear in the Log
  panel.
- **Selection panel** — subscribes to `term.onSelectionChange` and renders the
  current `[startRow,startCol] → [endRow,endCol]` range plus the selected
  text. Hold `Alt` while dragging to verify rectangular (column-aligned)
  selection.
- **Link panel** — subscribes to `term.onLink`. Every Cmd/Ctrl-click logs the
  URI + modifiers and calls `window.open(uri, '_blank', 'noopener')`.
- **Actions** — `copy()`, `serialize()` (downloads a `.ansi` blob),
  `scrollLines(99999)`, `scrollToBottom()`, and `Replay startup script`.

The fake PTY:

- Echoes printable input back to the terminal via `term.write()`
- Translates CR → CRLF and Backspace → destructive-backspace
- Logs every chunk it echoes so you can see exactly what `term.onInput`
  emitted (useful for confirming bracketed-paste wrappers and SGR mouse
  reports round-trip)

Loads config via the `configContents` option (`theme = nord`, JetBrains Mono
14) so the theme and font come from the same TOML path the desktop app uses.

## Run

```sh
bun install
bun run dev
```

Then open the URL printed by Vite (usually <http://localhost:5173>).

`libtermy.js` is consumed as a local `file:` dependency — the prebuilt
`dist/` and embedded WASM in the sibling package are used directly, so you
don't need to publish anything.

## Files

- `index.html` — page chrome with the top-bar controls, terminal host,
  selection panel, and sidebar markup
- `src/main.ts` — loads WASM once, owns the renderer-lifecycle slot
  (`teardown` + `rebuild`), registers the fake PTY + every sidebar handler,
  and streams the startup script
- `vite.config.ts` — Vite config (excludes `libtermy.js` from optimizeDeps so
  the embedded WASM module stays intact)

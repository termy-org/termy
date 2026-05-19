# libtermy.js

Browser WASM build of libtermy with a first-party canvas renderer.

```sh
npm install libtermy.js
```

```ts
import { loadTermy, createTermyRenderer } from 'libtermy.js'

const termy = await loadTermy()
const renderer = createTermyRenderer(document.querySelector('#terminal')!, {
  termy,
})

renderer.onInput((data) => {
  socket.send(data)
})

socket.addEventListener('message', (event) => {
  renderer.write(event.data)
})
```

`loadTermy()` is self-contained by default. The npm package embeds the generated
WASM bytes into `dist/index.js`, so apps do not need to copy `.wasm` files,
configure public asset paths, or pass a `wasmUrl`.

`createTermyRenderer` owns rendering, keyboard input, selection, scrollback,
and URL detection in a single pass. It supports `canvas2d` and `headless`
backends today; WebGL and WebGPU backends will land in a future release.

## Migration: from xterm.js to first-party rendering

The legacy `attachTermyToXterm` bridge double-parses every byte — once through
xterm.js to render and again through `core.feed()` to keep libtermy's snapshot
and search index in sync. The first-party `createTermyRenderer` API does both
in one pass, removes the `@xterm/xterm` dependency, and exposes richer
events (selection, links, resize) on a single object.

```ts
// Before — xterm.js adapter (deprecated, double-parses input)
import { Terminal } from '@xterm/xterm'
import { attachTermyToXterm, loadTermy } from 'libtermy.js'

const xterm = new Terminal()
xterm.open(document.querySelector('#terminal')!)

const termy = await loadTermy()
const bridge = attachTermyToXterm(xterm, {
  termy,
  onInput(data) {
    socket.send(data)
  },
})

socket.addEventListener('message', (event) => {
  bridge.write(event.data)
})
```

```ts
// After — first-party renderer (single-pass, no xterm.js needed)
import { createTermyRenderer, loadTermy } from 'libtermy.js'

const termy = await loadTermy()
const renderer = createTermyRenderer(document.querySelector('#terminal')!, {
  termy,
})

renderer.onInput((data) => {
  // data is Uint8Array — encode if your transport expects strings
  socket.send(data)
})

socket.addEventListener('message', (event) => {
  renderer.write(event.data)
})
```

Key differences:

- `renderer.onInput` yields `Uint8Array`, not `string`. Use a `TextDecoder` if
  your transport requires UTF-8 strings.
- `renderer` exposes `onResize`, `onSelectionChange`, `onLink`, `fit`,
  `getSelection`, `copy`, `paste`, `scrollToBottom`, and `serialize` directly.
- Drop the `@xterm/xterm` dependency and any xterm theme wiring; pass
  `configContents` or `renderConfig` to `createTermyRenderer` instead.

## Legacy: xterm.js adapter

> **Deprecated.** `attachTermyToXterm` is retained for existing integrations
> but double-parses input and will emit a one-time `console.warn` on first
> use. Migrate to `createTermyRenderer` (see above). Pass
> `{ silenceDeprecation: true }` to suppress the warning in test environments.

```sh
npm install libtermy.js @xterm/xterm
```

```ts
import { Terminal } from '@xterm/xterm'
import { attachTermyToXterm, loadTermy } from 'libtermy.js'

const xterm = new Terminal()
xterm.open(document.querySelector('#terminal')!)

const termy = await loadTermy()
const bridge = attachTermyToXterm(xterm, {
  termy,
  onInput(data) {
    socket.send(data)
  },
})

socket.addEventListener('message', (event) => {
  bridge.write(event.data)
})
```

`bridge.write(...)` writes data into xterm and feeds the same bytes through the
WASM parser. The returned feed result includes runtime events and terminal
responses that should be sent back to the backing PTY.

## Build

Build from this package directory:

```sh
bun install
bun run build
```

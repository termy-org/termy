# libtermy.js

Browser WASM build of libtermy with a small xterm.js adapter.

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

`loadTermy()` is self-contained by default. The npm package embeds the generated
WASM bytes into `dist/index.js`, so apps do not need to copy `.wasm` files,
configure public asset paths, or pass a `wasmUrl`.

`bridge.write(...)` writes data into xterm and feeds the same bytes through the
WASM parser. The returned feed result includes runtime events and terminal
responses that should be sent back to the backing PTY.

Build from this package directory:

```sh
bun install
bun run build
```

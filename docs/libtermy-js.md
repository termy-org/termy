# libtermy.js

`libtermy.js` is the browser package for embedding Termy's terminal parser and
config/theme semantics with xterm.js.

The package lives in `packages/libtermy-js` and is scaffolded with
`bun create tsdown@latest`. Its build runs two steps:

1. `wasm-pack build` compiles `crates/wasm` for `wasm32-unknown-unknown`.
2. `tsdown` builds the TypeScript browser library and declaration files.

```sh
cd packages/libtermy-js
bun install
bun run build
```

The Rust WASM crate intentionally does not spawn a PTY. Browser terminals should
connect xterm.js to a backend PTY over WebSocket or another transport, then feed
the same output bytes through the WASM parser.

```ts
import { Terminal } from '@xterm/xterm'
import { attachTermyToXterm, loadTermy } from 'libtermy.js'

const xterm = new Terminal()
const termy = await loadTermy()

const bridge = attachTermyToXterm(xterm, {
  termy,
  onInput(data) {
    socket.send(data)
  },
})

socket.addEventListener('message', (event) => {
  const result = bridge.write(event.data)
  for (const response of result.responses) {
    socket.send(response)
  }
})
```

`bridge.write(...)` writes to xterm and feeds the bytes into libtermy. The WASM
core exposes snapshots, visible-frame search, Termy config parsing, resolved
theme colors, and terminal response bytes for queries such as cursor position.

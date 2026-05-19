# libtermy.js

`libtermy.js` is the browser package for embedding Termy's terminal parser and
config/theme semantics with xterm.js.

The package lives in `packages/libtermy-js` and is scaffolded with
`bun create tsdown@latest`. Its build runs three steps:

1. `wasm-pack build` compiles `crates/wasm` for `wasm32-unknown-unknown`.
2. `scripts/embed-wasm.mjs` turns the generated `.wasm` into an internal
   TypeScript module.
3. `tsdown` bundles the TypeScript browser library, the wasm-bindgen JS glue,
   the embedded WASM bytes, and declaration files.

```sh
cd packages/libtermy-js
bun install
bun run build
```

`npm pack` and `npm publish` run the build automatically through `prepack` /
`prepublishOnly`, so the published package cannot accidentally miss the current
WASM output.

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

`loadTermy()` is the zero-config path. It initializes from the embedded WASM
bytes in `dist/index.js`, so consumers do not need a bundler plugin, a public
`.wasm` copy step, a custom MIME route, or a manual `wasmUrl`.

`bridge.write(...)` writes to xterm and feeds the bytes into libtermy. The WASM
core exposes snapshots, visible-frame search, Termy config parsing, resolved
theme colors, and terminal response bytes for queries such as cursor position.

Advanced embedders can still pass `moduleUrl` or `wasmUrl` to load custom
wasm-bindgen assets, but that is no longer required for normal npm installs.

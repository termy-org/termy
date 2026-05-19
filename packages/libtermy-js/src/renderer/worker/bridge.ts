// Main-thread bridge that exposes a TermyCore-compatible facade backed by a
// Web Worker running the WASM core. See ./worker.ts for the other side of the
// protocol.
//
// Strategy A from the spec: snapshot-cache + fire-and-forget mutation. We
// keep a `cachedSnapshot` on the main thread that the renderer reads
// synchronously, and the worker pushes a fresh snapshot whenever the core
// mutates. Query APIs that the synchronous `TermyCore` interface declares
// (`search`, `hyperlinkAt`, `encodeMouseReport`) are answered locally from
// the cached frame; for `search` we kick off an async query in the worker so
// that the next call returns up-to-date results.

import type {
  LoadedTermy,
  MouseEncoding,
  MouseMode,
  TermyCell,
  TermyCore,
  TermyFeedResult,
  TermyFrame,
  TermyRenderConfig,
  TermyRuntimeEvent,
  TermySearchMatch,
} from '../../index'

type WorkerInbound =
  | {
      kind: 'init'
      cols: number
      rows: number
      cellWidth: number
      cellHeight: number
      configContents?: string
      scrollback?: number
    }
  | { kind: 'feed'; payload: Uint8Array }
  | {
      kind: 'resize'
      cols: number
      rows: number
      cellWidth: number
      cellHeight: number
    }
  | { kind: 'setConfigContents'; contents: string }
  | { kind: 'setScrollback'; budget: number }
  | { kind: 'scrollLines'; amount: number }
  | { kind: 'scrollToBottom' }
  | { kind: 'search'; query: string; nonce: number }
  | {
      kind: 'encodeMouseReport'
      button: number
      modifiers: number
      col: number
      row: number
      eventKind: number
      nonce: number
    }
  | { kind: 'hyperlinkAt'; row: number; col: number; nonce: number }
  | { kind: 'dispose' }

type WorkerOutbound =
  | { kind: 'ready' }
  | {
      kind: 'snapshot'
      frame: TermyFrame
      responses: string[]
      events: TermyRuntimeEvent[]
    }
  | { kind: 'searchResult'; nonce: number; matches: TermySearchMatch[] }
  | {
      kind: 'encodeMouseReportResult'
      nonce: number
      bytes: Uint8Array | null
    }
  | { kind: 'hyperlinkAtResult'; nonce: number; uri: string | null }
  | { kind: 'error'; message: string }

export interface CreateWorkerCoreBridgeOptions {
  termy: LoadedTermy
  cols: number
  rows: number
  cellWidth: number
  cellHeight: number
  configContents?: string
  scrollback?: number
}

export interface SnapshotEvent {
  frame: TermyFrame
  responses: string[]
  events: TermyRuntimeEvent[]
}

export type SnapshotListener = (event: SnapshotEvent) => void

export interface WorkerCoreBridge extends TermyCore {
  /**
   * Whether this core is backed by a worker. Always `true` for bridges
   * returned by {@link createWorkerCoreBridge}; the renderer factory uses
   * this to decide whether to subscribe for snapshot push notifications.
   */
  readonly isWorkerBridge: true
  /**
   * Subscribe to snapshot pushes from the worker. The callback fires after
   * every state-mutating worker call (feed, resize, scroll, …). The
   * `responses` and `events` arrays correspond to the most recent core
   * mutation that produced this snapshot.
   *
   * Returns an unsubscribe function.
   */
  subscribeToSnapshot(listener: SnapshotListener): () => void
  /** Best-effort: returns a promise that resolves once the worker reports `ready`. */
  ready(): Promise<void>
  /** Tear down the worker and release all internal listeners. */
  dispose(): void
}

const EMPTY_FRAME: TermyFrame = {
  cols: 0,
  rows: 0,
  cells: [],
  cursor: null,
  displayOffset: 0,
  historySize: 0,
  applicationCursorKeys: false,
  mouseMode: 'none',
  mouseEncoding: 'legacy',
  bracketedPaste: false,
  hyperlinks: [],
}

const EMPTY_FEED_RESULT: TermyFeedResult = { events: [], responses: [] }

export function createWorkerCoreBridge(
  options: CreateWorkerCoreBridgeOptions,
): WorkerCoreBridge {
  // The worker chunk is emitted to `dist/worker.js` (see tsdown.config.ts).
  // We reference the .js sibling so the URL resolves correctly relative to
  // `dist/index.js` at runtime. Most modern bundlers (Vite, webpack 5+,
  // Parcel, esbuild) detect the `new Worker(new URL(..., import.meta.url))`
  // pattern and rewrite the URL during their own bundle pass.
  const worker = new Worker(new URL('./worker.js', import.meta.url), { type: 'module' })

  let cachedSnapshot: TermyFrame | null = null
  let cachedRenderConfig: TermyRenderConfig | null = null
  let cachedSearchMatches: TermySearchMatch[] = []
  let lastSearchQuery: string | null = null
  let nonceCounter = 0
  let disposed = false

  // Responses are emitted to the renderer one feed-round late. We buffer
  // responses from each snapshot post and flush them out of the *next*
  // feed/drain call so that callers reading `result.responses` still see
  // recently-arrived data even though the synchronous round-trip can't
  // observe the response for the in-flight bytes.
  const pendingResponses: string[] = []
  const pendingEvents: TermyRuntimeEvent[] = []

  const snapshotListeners = new Set<SnapshotListener>()
  const pendingSearch = new Map<
    number,
    (matches: TermySearchMatch[]) => void
  >()

  let readyResolve: () => void = () => {}
  const readyPromise = new Promise<void>((resolve) => {
    readyResolve = resolve
  })

  function send(message: WorkerInbound, transfer: Transferable[] = []): void {
    if (disposed) return
    worker.postMessage(message, transfer)
  }

  function deriveRenderConfig(contents: string): TermyRenderConfig {
    if (contents && options.termy?.renderConfigFromContents) {
      return options.termy.renderConfigFromContents(contents)
    }
    return options.termy.defaultRenderConfig()
  }

  worker.addEventListener('message', (event: MessageEvent<WorkerOutbound>) => {
    const data = event.data
    if (!data || typeof data !== 'object') return

    switch (data.kind) {
      case 'ready':
        readyResolve()
        break
      case 'snapshot': {
        cachedSnapshot = data.frame
        if (data.responses?.length) {
          pendingResponses.push(...data.responses)
        }
        if (data.events?.length) {
          pendingEvents.push(...data.events)
        }
        // If a search query is active, re-issue it so cached matches stay
        // approximately current. (cheap on small terminals; large outputs
        // may want to debounce — left to consumer.)
        if (lastSearchQuery !== null) {
          send({ kind: 'search', query: lastSearchQuery, nonce: ++nonceCounter })
        }
        for (const listener of snapshotListeners) {
          listener({
            frame: data.frame,
            responses: data.responses ?? [],
            events: data.events ?? [],
          })
        }
        break
      }
      case 'searchResult': {
        cachedSearchMatches = data.matches ?? []
        const resolver = pendingSearch.get(data.nonce)
        if (resolver) {
          pendingSearch.delete(data.nonce)
          resolver(cachedSearchMatches)
        }
        break
      }
      case 'encodeMouseReportResult':
      case 'hyperlinkAtResult':
        // The bridge's `TermyCore` facade answers these synchronously from
        // the cached snapshot (see encodeMouseReportLocal / hyperlinkAtLocal
        // below); the worker-side responses are reserved for future
        // SharedArrayBuffer-based strict round-trip mode. Ignore for now.
        break
      case 'error':
        if (typeof console !== 'undefined') {
          console.error('[libtermy.js worker]', data.message)
        }
        break
    }
  })

  send({
    kind: 'init',
    cols: options.cols,
    rows: options.rows,
    cellWidth: options.cellWidth,
    cellHeight: options.cellHeight,
    configContents: options.configContents,
    scrollback: options.scrollback,
  })

  function drainPending(): TermyFeedResult {
    if (pendingResponses.length === 0 && pendingEvents.length === 0) {
      return EMPTY_FEED_RESULT
    }
    const responses = pendingResponses.splice(0)
    const events = pendingEvents.splice(0)
    return { responses, events }
  }

  function currentFrame(): TermyFrame {
    return cachedSnapshot ?? EMPTY_FRAME
  }

  // JS port of `encode_mouse_packet` + `event_allowed` from crates/wasm/src/lib.rs.
  // Implements Legacy + SGR. UTF-8 and SGR-Pixel encodings are best-effort:
  // we fall through to Legacy for UTF-8 (which differs only for coords >=95)
  // and to SGR for SGR-Pixel (pixel-precision is lost — coords stay cell-
  // indexed). For a workerized core most consumers won't notice; strict
  // parity callers should disable workerized mode.
  function encodeMouseReportLocal(
    button: number,
    modifiers: number,
    col: number,
    row: number,
    kind: number,
  ): Uint8Array | undefined {
    const mode = currentFrame().mouseMode
    if (!eventAllowed(mode, kind)) return undefined
    const encoding = currentFrame().mouseEncoding
    return encodeMousePacket(encoding, button, modifiers, col, row, kind)
  }

  function hyperlinkAtLocal(row: number, col: number): string | undefined {
    const frame = currentFrame()
    if (
      frame.cols === 0 ||
      frame.rows === 0 ||
      row < 0 ||
      col < 0 ||
      row >= frame.rows ||
      col >= frame.cols
    ) {
      return undefined
    }
    const cell: TermyCell | undefined = frame.cells[row * frame.cols + col]
    if (!cell || cell.hyperlinkId === null || cell.hyperlinkId === undefined) {
      return undefined
    }
    return frame.hyperlinks[cell.hyperlinkId]
  }

  const bridge: WorkerCoreBridge = {
    isWorkerBridge: true,

    ready(): Promise<void> {
      return readyPromise
    },

    subscribeToSnapshot(listener: SnapshotListener): () => void {
      snapshotListeners.add(listener)
      // Replay the current snapshot immediately if we have one so subscribers
      // don't miss the initial frame.
      if (cachedSnapshot) {
        listener({ frame: cachedSnapshot, responses: [], events: [] })
      }
      return () => {
        snapshotListeners.delete(listener)
      }
    },

    resize(cols: number, rows: number, cellWidth: number, cellHeight: number): void {
      send({ kind: 'resize', cols, rows, cellWidth, cellHeight })
    },

    setConfigContents(contents: string): TermyRenderConfig {
      // Compute the render config synchronously on the main thread using the
      // same WASM module the user already loaded via `loadTermy`. Then ship
      // the contents to the worker so its core state stays in sync.
      const config = deriveRenderConfig(contents)
      cachedRenderConfig = config
      send({ kind: 'setConfigContents', contents })
      return config
    },

    feed(bytes: Uint8Array): TermyFeedResult {
      // Transfer the underlying buffer so we don't pay a copy on every
      // keystroke. After transfer the caller's reference is detached, but
      // callers conventionally don't reuse the buffer they pass to feed.
      // If the buffer can't be transferred (e.g. it's a view onto a SAB),
      // postMessage will copy instead.
      const transfer: Transferable[] = []
      if (bytes.buffer && bytes.byteOffset === 0 && bytes.byteLength === bytes.buffer.byteLength) {
        transfer.push(bytes.buffer)
        send({ kind: 'feed', payload: bytes }, transfer)
      } else {
        // Defensive copy when the view isn't a tight fit (slices etc).
        const copy = new Uint8Array(bytes)
        send({ kind: 'feed', payload: copy }, [copy.buffer])
      }
      return drainPending()
    },

    drain(): TermyFeedResult {
      return drainPending()
    },

    snapshot(): TermyFrame {
      return currentFrame()
    },

    search(query: string): TermySearchMatch[] {
      // Synchronous facade: return the last known matches and schedule a
      // fresh async query in the background. The next call will see updated
      // results. Consumers that need strict per-call freshness should use the
      // future `searchAsync` API on the bridge (TODO).
      lastSearchQuery = query
      const nonce = ++nonceCounter
      send({ kind: 'search', query, nonce })
      return cachedSearchMatches
    },

    setScrollback(budget: number): void {
      send({ kind: 'setScrollback', budget })
    },

    scrollLines(amount: number): void {
      send({ kind: 'scrollLines', amount })
    },

    scrollToBottom(): void {
      send({ kind: 'scrollToBottom' })
    },

    displayOffset(): number {
      return currentFrame().displayOffset
    },

    historySize(): number {
      return currentFrame().historySize
    },

    applicationCursorKeys(): boolean {
      return currentFrame().applicationCursorKeys
    },

    mouseMode(): MouseMode {
      return currentFrame().mouseMode
    },

    mouseEncoding(): MouseEncoding {
      return currentFrame().mouseEncoding
    },

    bracketedPaste(): boolean {
      return currentFrame().bracketedPaste
    },

    hyperlinkAt(row: number, col: number): string | undefined {
      return hyperlinkAtLocal(row, col)
    },

    encodeMouseReport(
      button: number,
      modifiers: number,
      col: number,
      row: number,
      kind: number,
    ): Uint8Array | undefined {
      return encodeMouseReportLocal(button, modifiers, col, row, kind)
    },

    dispose(): void {
      if (disposed) return
      disposed = true
      send({ kind: 'dispose' })
      worker.terminate()
      snapshotListeners.clear()
      pendingSearch.clear()
    },
  }

  // Surface cachedRenderConfig usage so the linter doesn't strip the assignment.
  void cachedRenderConfig

  return bridge
}

// --- JS-side mouse encoding (port of crates/wasm/src/lib.rs) ----------------

function eventAllowed(mode: MouseMode, kind: number): boolean {
  switch (mode) {
    case 'none':
      return false
    case 'x10':
      return kind === 0 // Press only
    case 'normal':
      return kind === 0 || kind === 1 // Press + Release
    case 'button-event':
      return kind === 0 || kind === 1 || kind === 2 // Press + Release + Drag
    case 'any-event':
      return true
    default:
      return false
  }
}

function encodeMousePacket(
  encoding: MouseEncoding,
  button: number,
  modifiers: number,
  col: number,
  row: number,
  kind: number,
): Uint8Array | undefined {
  const motionBit = kind === 2 || kind === 3 ? 32 : 0

  if (encoding === 'sgr' || encoding === 'sgr-pixel') {
    const encodedButton = button + motionBit + modifiers
    const suffix = kind === 1 ? 'm' : 'M'
    const colValue = col + 1
    const rowValue = row + 1
    const str = `\x1b[<${encodedButton};${colValue};${rowValue}${suffix}`
    return textEncoder.encode(str)
  }

  // Legacy + UTF-8. UTF-8 is a superset that allows coords >= 95 to span two
  // bytes; we implement both paths.
  const utf8 = encoding === 'utf8'
  const maxPoint = utf8 ? 2015 : 223
  if (col >= maxPoint || row >= maxPoint) {
    return undefined
  }

  // Release events collapse to button code 3 in legacy/utf8 encodings.
  const baseButton = kind === 1 ? 3 : button
  const encodedButton = baseButton + motionBit + modifiers
  const buttonByte = Math.min(255, 32 + encodedButton)

  const out: number[] = [0x1b, 0x5b, 0x4d, buttonByte] // ESC [ M <btn>
  pushCoordinate(out, col, utf8)
  pushCoordinate(out, row, utf8)
  return new Uint8Array(out)
}

function pushCoordinate(out: number[], value: number, utf8: boolean): void {
  const encoded = 32 + 1 + value
  if (utf8 && encoded >= 0x80) {
    const first = 0xc0 + Math.floor(encoded / 64)
    const second = 0x80 + (encoded & 0x3f)
    out.push(first, second)
  } else {
    out.push(Math.min(255, encoded))
  }
}

const textEncoder = new TextEncoder()

/// <reference lib="webworker" />
//
// libtermy.js Web Worker entry point.
//
// Runs the WASM terminal core off the main thread. The bridge on the main
// thread (see ./bridge.ts) drives this worker via the `WorkerInbound` message
// protocol. On every state-mutating call we re-snapshot and post the new
// frame back so the bridge can update its cached snapshot.
//
// WASM loading strategy:
//   1. Try to use the embedded base64 blob (../../generated/wasm-bytes.ts).
//      This is what `loadTermy()` uses on the main thread and is the most
//      reliable path across bundlers.
//   2. If the embedded import fails (e.g. bundler can't ship the blob into
//      the worker chunk), fall back to fetching termy_wasm_bg.wasm via the
//      same URL that the generated glue module ships with.

import type { TermyCore, TermyFeedResult } from '../../index'
import type {
  TermyFrame,
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

const ctx = self as unknown as DedicatedWorkerGlobalScope

let core: TermyCore | null = null
let initializing: Promise<TermyCore> | null = null

function post(message: WorkerOutbound, transfer: Transferable[] = []): void {
  // The DedicatedWorkerGlobalScope.postMessage signature accepts a transfer
  // list; cast through unknown for TS lib variance across targets.
  ;(ctx.postMessage as (m: unknown, t?: Transferable[]) => void)(message, transfer)
}

async function loadCore(
  cols: number,
  rows: number,
  cellWidth: number,
  cellHeight: number,
): Promise<TermyCore> {
  // Dynamic imports keep the WASM glue out of the bridge module's graph;
  // some bundlers (e.g. tsdown / rolldown in worker mode) refuse to ship a
  // top-level static import to a chunk that doesn't reference them.
  const wasmModuleNs = (await import('../../generated/termy-wasm.js')) as unknown as {
    default: (input?: unknown) => Promise<unknown>
    TermyTerminal: {
      withCellSize(
        cols: number,
        rows: number,
        cellWidth: number,
        cellHeight: number,
      ): TermyCore
    }
  }

  let initialized = false
  try {
    const { embeddedWasmBytes } = (await import(
      '../../generated/wasm-bytes'
    )) as { embeddedWasmBytes: () => Uint8Array }
    await wasmModuleNs.default({ module_or_path: embeddedWasmBytes() })
    initialized = true
  } catch {
    // The embedded blob path failed (likely because the bundler stripped the
    // base64 from the worker chunk). Fall back to URL-based fetch.
  }

  if (!initialized) {
    // Resolved relative to the *bundled* worker file: dist/worker.js. The
    // package ships `wasm/termy_wasm_bg.wasm` at the package root, so the
    // sibling path from `dist/` is one level up.
    //
    // In source-mode this URL is wrong (worker.ts lives at
    // src/renderer/worker/worker.ts) but the embedded-blob branch above
    // succeeds first in that scenario, so we only reach this fallback in
    // bundled-production contexts where the path is correct.
    const wasmUrl = new URL('../wasm/termy_wasm_bg.wasm', import.meta.url)
    await wasmModuleNs.default({ module_or_path: wasmUrl })
  }

  return wasmModuleNs.TermyTerminal.withCellSize(cols, rows, cellWidth, cellHeight)
}

function snapshotAndPost(extraResponses?: string[], extraEvents?: TermyRuntimeEvent[]): void {
  if (!core) return
  const frame = core.snapshot()
  post({
    kind: 'snapshot',
    frame,
    responses: extraResponses ?? [],
    events: extraEvents ?? [],
  })
}

async function handleInit(message: Extract<WorkerInbound, { kind: 'init' }>): Promise<void> {
  if (initializing) {
    await initializing
    return
  }
  initializing = loadCore(message.cols, message.rows, message.cellWidth, message.cellHeight)
  try {
    core = await initializing
  } catch (err) {
    initializing = null
    post({ kind: 'error', message: (err as Error)?.message ?? String(err) })
    return
  }

  if (message.scrollback !== undefined) {
    core.setScrollback(message.scrollback)
  }
  if (message.configContents) {
    try {
      core.setConfigContents(message.configContents)
    } catch (err) {
      post({ kind: 'error', message: (err as Error)?.message ?? String(err) })
    }
  }

  post({ kind: 'ready' })
  snapshotAndPost()
}

function handleFeed(payload: Uint8Array): void {
  if (!core) return
  try {
    const result = core.feed(payload) as TermyFeedResult | undefined
    snapshotAndPost(result?.responses ?? [], result?.events ?? [])
  } catch (err) {
    post({ kind: 'error', message: (err as Error)?.message ?? String(err) })
  }
}

function handleResize(message: Extract<WorkerInbound, { kind: 'resize' }>): void {
  if (!core) return
  core.resize(message.cols, message.rows, message.cellWidth, message.cellHeight)
  snapshotAndPost()
}

function handleSetConfigContents(contents: string): void {
  if (!core) return
  try {
    core.setConfigContents(contents)
    snapshotAndPost()
  } catch (err) {
    post({ kind: 'error', message: (err as Error)?.message ?? String(err) })
  }
}

function handleSetScrollback(budget: number): void {
  if (!core) return
  core.setScrollback(budget)
  snapshotAndPost()
}

function handleScrollLines(amount: number): void {
  if (!core) return
  core.scrollLines(amount)
  snapshotAndPost()
}

function handleScrollToBottom(): void {
  if (!core) return
  core.scrollToBottom()
  snapshotAndPost()
}

function handleSearch(query: string, nonce: number): void {
  if (!core) {
    post({ kind: 'searchResult', nonce, matches: [] })
    return
  }
  try {
    const matches = (core.search(query) as TermySearchMatch[]) ?? []
    post({ kind: 'searchResult', nonce, matches })
  } catch (err) {
    post({ kind: 'error', message: (err as Error)?.message ?? String(err) })
    post({ kind: 'searchResult', nonce, matches: [] })
  }
}

function handleEncodeMouseReport(
  message: Extract<WorkerInbound, { kind: 'encodeMouseReport' }>,
): void {
  if (!core) {
    post({ kind: 'encodeMouseReportResult', nonce: message.nonce, bytes: null })
    return
  }
  const bytes = core.encodeMouseReport(
    message.button,
    message.modifiers,
    message.col,
    message.row,
    message.eventKind,
  )
  const payload = bytes ?? null
  const transfer: Transferable[] = []
  if (payload) {
    transfer.push(payload.buffer)
  }
  post(
    {
      kind: 'encodeMouseReportResult',
      nonce: message.nonce,
      bytes: payload,
    },
    transfer,
  )
}

function handleHyperlinkAt(row: number, col: number, nonce: number): void {
  if (!core) {
    post({ kind: 'hyperlinkAtResult', nonce, uri: null })
    return
  }
  const uri = core.hyperlinkAt(row, col)
  post({ kind: 'hyperlinkAtResult', nonce, uri: uri ?? null })
}

function handleDispose(): void {
  core = null
  initializing = null
}

ctx.addEventListener('message', (event: MessageEvent<WorkerInbound>) => {
  const data = event.data
  if (!data || typeof data !== 'object') return

  switch (data.kind) {
    case 'init':
      void handleInit(data)
      break
    case 'feed':
      handleFeed(data.payload)
      break
    case 'resize':
      handleResize(data)
      break
    case 'setConfigContents':
      handleSetConfigContents(data.contents)
      break
    case 'setScrollback':
      handleSetScrollback(data.budget)
      break
    case 'scrollLines':
      handleScrollLines(data.amount)
      break
    case 'scrollToBottom':
      handleScrollToBottom()
      break
    case 'search':
      handleSearch(data.query, data.nonce)
      break
    case 'encodeMouseReport':
      handleEncodeMouseReport(data)
      break
    case 'hyperlinkAt':
      handleHyperlinkAt(data.row, data.col, data.nonce)
      break
    case 'dispose':
      handleDispose()
      break
    default: {
      const exhaustive: never = data
      void exhaustive
    }
  }
})

export {}

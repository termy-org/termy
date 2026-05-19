import type { TermyCore, TermyFeedResult, TermyFrame, TermySearchMatch } from '../../index'
import {
  createLifecycleDispatchers,
  dispatchTermyEvents,
  type LifecycleEventDispatchers,
} from '../events'
import { SearchHighlightTracker } from '../search-highlight'
import { serializeFrameToAnsi } from '../serialize'
import type {
  CreateTermyRendererOptions,
  Disposable,
  LinkPayload,
  ProgressPayload,
  ResizePayload,
  SearchOptions,
  SelectionPayload,
  TermyRenderer,
  TermyRendererBackend,
} from '../types'

type Listener<T> = (value: T) => void

const TEXT_ENCODER = new TextEncoder()

interface HeadlessRendererInit {
  core: TermyCore
  cols: number
  rows: number
  cellWidth: number
  cellHeight: number
  backend: TermyRendererBackend
}

export function createHeadlessRenderer(init: HeadlessRendererInit): TermyRenderer {
  const inputListeners = new Set<Listener<Uint8Array>>()
  const resizeListeners = new Set<Listener<ResizePayload>>()
  const selectionListeners = new Set<Listener<SelectionPayload>>()
  const linkListeners = new Set<Listener<LinkPayload>>()
  const lifecycleDispatchers: LifecycleEventDispatchers = createLifecycleDispatchers()

  let cols = init.cols
  let rows = init.rows
  const cellWidth = init.cellWidth
  const cellHeight = init.cellHeight
  const searchHighlight = new SearchHighlightTracker()
  let disposed = false

  function ensureNotDisposed(): void {
    if (disposed) {
      throw new Error('TermyRenderer has been disposed')
    }
  }

  function fireInput(payload: Uint8Array): void {
    for (const listener of inputListeners) {
      listener(payload)
    }
  }

  return {
    core: init.core,
    backend: init.backend,
    get cols() {
      return cols
    },
    get rows() {
      return rows
    },

    write(data) {
      ensureNotDisposed()
      const bytes = typeof data === 'string' ? TEXT_ENCODER.encode(data) : data
      const result = init.core.feed(bytes) as TermyFeedResult
      if (result?.responses) {
        for (const response of result.responses) {
          fireInput(TEXT_ENCODER.encode(response))
        }
      }
      if (result?.events) {
        dispatchTermyEvents(result.events, lifecycleDispatchers)
      }
      return result
    },

    resize(nextCols, nextRows) {
      ensureNotDisposed()
      cols = nextCols ?? cols
      rows = nextRows ?? rows
      init.core.resize(cols, rows, cellWidth, cellHeight)
      const payload: ResizePayload = { cols, rows }
      for (const listener of resizeListeners) {
        listener(payload)
      }
    },

    fit() {
      return { cols, rows }
    },

    focus() {},
    blur() {},
    scrollToBottom() {
      init.core.scrollToBottom()
    },
    scrollLines(amount: number) {
      init.core.scrollLines(amount)
    },
    getSelection() {
      return ''
    },
    clearSelection() {},

    async copy() {
      const text = ''
      if (typeof navigator !== 'undefined' && navigator.clipboard) {
        await navigator.clipboard.writeText(text)
      }
    },

    paste(text) {
      ensureNotDisposed()
      fireInput(TEXT_ENCODER.encode(text))
    },

    snapshot(): TermyFrame {
      ensureNotDisposed()
      return init.core.snapshot()
    },

    search(query, _options?: SearchOptions) {
      ensureNotDisposed()
      return init.core.search(query)
    },

    searchAndHighlight(query: string, _options?: SearchOptions): TermySearchMatch[] {
      ensureNotDisposed()
      const matches = init.core.search(query)
      searchHighlight.setQuery(query, matches)
      return matches
    },

    findNextMatch(): TermySearchMatch | null {
      ensureNotDisposed()
      return searchHighlight.next()
    },

    findPreviousMatch(): TermySearchMatch | null {
      ensureNotDisposed()
      return searchHighlight.previous()
    },

    clearSearchHighlight(): void {
      ensureNotDisposed()
      searchHighlight.clear()
    },

    serialize() {
      ensureNotDisposed()
      return serializeFrameToAnsi(init.core.snapshot())
    },

    onInput(listener): Disposable {
      inputListeners.add(listener)
      return { dispose: () => inputListeners.delete(listener) }
    },
    onResize(listener): Disposable {
      resizeListeners.add(listener)
      return { dispose: () => resizeListeners.delete(listener) }
    },
    onSelectionChange(listener): Disposable {
      selectionListeners.add(listener)
      return { dispose: () => selectionListeners.delete(listener) }
    },
    onLink(listener): Disposable {
      linkListeners.add(listener)
      return { dispose: () => linkListeners.delete(listener) }
    },
    onTitle(listener): Disposable {
      lifecycleDispatchers.title.add(listener)
      return { dispose: () => lifecycleDispatchers.title.delete(listener) }
    },
    onWorkingDirectory(listener): Disposable {
      lifecycleDispatchers.workingDirectory.add(listener)
      return { dispose: () => lifecycleDispatchers.workingDirectory.delete(listener) }
    },
    onProgress(listener): Disposable {
      lifecycleDispatchers.progress.add(listener)
      return { dispose: () => lifecycleDispatchers.progress.delete(listener) }
    },
    onBell(listener): Disposable {
      lifecycleDispatchers.bell.add(listener)
      return { dispose: () => lifecycleDispatchers.bell.delete(listener) }
    },
    onClipboardStore(listener): Disposable {
      lifecycleDispatchers.clipboardStore.add(listener)
      return { dispose: () => lifecycleDispatchers.clipboardStore.delete(listener) }
    },

    dispose() {
      if (disposed) return
      disposed = true
      inputListeners.clear()
      resizeListeners.clear()
      selectionListeners.clear()
      linkListeners.clear()
      lifecycleDispatchers.title.clear()
      lifecycleDispatchers.workingDirectory.clear()
      lifecycleDispatchers.progress.clear()
      lifecycleDispatchers.bell.clear()
      lifecycleDispatchers.clipboardStore.clear()
    },
  }
}

export function isHeadlessHost(
  host: HTMLElement | null | undefined,
  options: CreateTermyRendererOptions,
): boolean {
  return host === null || options.backend === 'headless'
}

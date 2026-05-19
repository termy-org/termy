import type {
  LoadedTermy,
  TermyCore,
  TermyFeedResult,
  TermyFrame,
  TermyRenderConfig,
  TermySearchMatch,
} from '../index'

export type TermyRendererBackend = 'webgl' | 'webgpu' | 'canvas2d' | 'headless' | 'auto'

export type BellMode = 'none' | 'visual' | 'audio'

export interface Disposable {
  dispose(): void
}

export interface SearchOptions {
  caseSensitive?: boolean
  wholeWord?: boolean
  regex?: boolean
  limit?: number
}

export interface LinkPayload {
  uri: string
  row: number
  startCol: number
  endCol: number
  modifiers: {
    control: boolean
    alt: boolean
    shift: boolean
    meta: boolean
  }
}

export interface SelectionPayload {
  active: boolean
  startRow: number
  startCol: number
  endRow: number
  endCol: number
  text: string
}

export interface ResizePayload {
  cols: number
  rows: number
}

/**
 * Parsed OSC 9;4 progress payload.
 *
 * The raw payload emitted by the WASM core is the full OSC param string
 * minus the leading `OSC 9;` prefix — i.e. `4;<state>;<value>`. The `state`
 * codes follow ConEmu's convention:
 *
 *   0 = no progress, 1 = normal, 2 = error,
 *   3 = indeterminate, 4 = paused
 *
 * `value` is clamped to 0..100. It is forced to `0` for the `none` and
 * `indeterminate` states (where the value is meaningless).
 */
export interface ProgressPayload {
  state: 'none' | 'normal' | 'error' | 'indeterminate' | 'paused'
  value: number
  raw: string
}

export interface CreateTermyRendererOptions {
  termy?: LoadedTermy
  core?: TermyCore
  configContents?: string
  renderConfig?: TermyRenderConfig
  backend?: TermyRendererBackend
  workerized?: boolean
  cols?: number
  rows?: number
  cellWidth?: number
  cellHeight?: number
  scrollback?: number
  rendererFps?: number
  cursorBlink?: boolean
  fontFamily?: string
  fontSize?: number
  lineHeight?: number
  letterSpacing?: number
  drawBoldTextInBrightColors?: boolean
  allowTransparency?: boolean
  rightClickSelectsWord?: boolean
  macOptionIsMeta?: boolean
  wordSeparator?: string
  bellSound?: BellMode
}

export interface TermyRenderer {
  readonly core: TermyCore
  readonly backend: TermyRendererBackend
  readonly cols: number
  readonly rows: number
  write(data: string | Uint8Array): TermyFeedResult
  resize(cols?: number, rows?: number): void
  fit(): ResizePayload
  focus(): void
  blur(): void
  scrollToBottom(): void
  scrollLines(amount: number): void
  getSelection(): string
  clearSelection(): void
  copy(): Promise<void>
  paste(text: string): void
  snapshot(): TermyFrame
  search(query: string, options?: SearchOptions): TermySearchMatch[]
  searchAndHighlight(query: string, options?: SearchOptions): TermySearchMatch[]
  findNextMatch(): TermySearchMatch | null
  findPreviousMatch(): TermySearchMatch | null
  clearSearchHighlight(): void
  serialize(): Uint8Array
  onInput(listener: (data: Uint8Array) => void): Disposable
  onResize(listener: (size: ResizePayload) => void): Disposable
  onSelectionChange(listener: (selection: SelectionPayload) => void): Disposable
  onLink(listener: (link: LinkPayload) => void): Disposable
  onTitle(listener: (title: string) => void): Disposable
  onWorkingDirectory(listener: (uri: string) => void): Disposable
  onProgress(listener: (payload: ProgressPayload) => void): Disposable
  onBell(listener: () => void): Disposable
  onClipboardStore(listener: (text: string) => void): Disposable
  dispose(): void
}

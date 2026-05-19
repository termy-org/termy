import { embeddedWasmBytes } from './generated/wasm-bytes'
import * as bundledWasmModule from './generated/termy-wasm.js'
import { createTermyRenderer } from './renderer'
import type { CreateTermyRendererOptions, TermyRenderer } from './renderer/types'

export type {
  CreateTermyRendererOptions,
  Disposable,
  LinkPayload,
  ResizePayload,
  SearchOptions,
  SelectionPayload,
  TermyRenderer,
  TermyRendererBackend,
  BellMode,
} from './renderer/types'
export type { Keystroke, KeyModifiers, TerminalKeyboardMode } from './renderer/keyboard'
export {
  createTermyRenderer,
  encodeKeystroke,
  DEFAULT_KEYBOARD_MODE,
  EMPTY_MODIFIERS,
  serializeFrameToAnsi,
} from './renderer'

export interface TermyColor {
  r: number
  g: number
  b: number
  a: number
}

export interface TermyCell {
  col: number
  row: number
  char: string
  fg: TermyColor
  bg: TermyColor
  usesTerminalDefaultBg: boolean
  bold: boolean
  italic: boolean
  underline: boolean
  strikethrough: boolean
  dim: boolean
  /**
   * Reverse video. The cell still carries the logical fg/bg colors; the
   * renderer is responsible for swapping them at paint time.
   */
  reverse: boolean
  blink: boolean
  invisible: boolean
  renderText: boolean
  hyperlinkId: number | null
  /**
   * Column width occupied by this cell.
   *
   * * `1` — normal narrow cell (default).
   * * `2` — wide cell (CJK / emoji / fullwidth). The glyph is painted
   *   spanning this column and the next.
   * * `0` — placeholder representing the right half of a wide glyph; the
   *   renderer should skip emitting a glyph for it.
   */
  width: number
}

export interface TermyCursor {
  col: number
  row: number
  style: 'line' | 'block'
}

export type MouseMode = 'none' | 'x10' | 'normal' | 'button-event' | 'any-event'
export type MouseEncoding = 'legacy' | 'sgr' | 'utf8' | 'sgr-pixel'

export interface TermyFrame {
  cols: number
  rows: number
  cells: TermyCell[]
  cursor: TermyCursor | null
  displayOffset: number
  historySize: number
  applicationCursorKeys: boolean
  mouseMode: MouseMode
  mouseEncoding: MouseEncoding
  bracketedPaste: boolean
  hyperlinks: string[]
}

export interface TermyRuntimeEvent {
  kind: string
  payload?: string
}

export interface TermyFeedResult {
  events: TermyRuntimeEvent[]
  responses: string[]
}

export interface TermySearchMatch {
  row: number
  startCol: number
  endCol: number
  line: string
}

export interface TermyConfigDiagnostic {
  lineNumber: number
  kind: string
  message: string
}

export interface TermyRenderConfig {
  activeTheme: string
  fontFamily: string
  fontSize: number
  lineHeight: number
  paddingX: number
  paddingY: number
  backgroundOpacity: number
  backgroundOpacityCells: boolean
  cursorBlink: boolean
  cursorStyle: 'line' | 'block'
  foreground: TermyColor
  background: TermyColor
  cursor: TermyColor
  ansi: [
    TermyColor,
    TermyColor,
    TermyColor,
    TermyColor,
    TermyColor,
    TermyColor,
    TermyColor,
    TermyColor,
    TermyColor,
    TermyColor,
    TermyColor,
    TermyColor,
    TermyColor,
    TermyColor,
    TermyColor,
    TermyColor,
  ]
  diagnostics: TermyConfigDiagnostic[]
}

export interface TermyCore {
  resize(cols: number, rows: number, cellWidth: number, cellHeight: number): void
  setConfigContents(contents: string): TermyRenderConfig
  feed(bytes: Uint8Array): TermyFeedResult
  drain(): TermyFeedResult
  snapshot(): TermyFrame
  search(query: string): TermySearchMatch[]
  setScrollback(budget: number): void
  scrollLines(amount: number): void
  scrollToBottom(): void
  displayOffset(): number
  historySize(): number
  applicationCursorKeys(): boolean
  mouseMode(): MouseMode
  mouseEncoding(): MouseEncoding
  bracketedPaste(): boolean
  hyperlinkAt(row: number, col: number): string | undefined
  encodeMouseReport(
    button: number,
    modifiers: number,
    col: number,
    row: number,
    kind: number,
  ): Uint8Array | undefined
}

export interface TermyWasmModule {
  default(input?: unknown): Promise<unknown>
  TermyTerminal: {
    new (cols: number, rows: number): TermyCore
    withCellSize(cols: number, rows: number, cellWidth: number, cellHeight: number): TermyCore
  }
  defaultRenderConfig(): TermyRenderConfig
  renderConfigFromContents(contents: string): TermyRenderConfig
}

export interface LoadTermyOptions {
  moduleUrl?: string | URL
  wasmUrl?: string | URL | Request | Response | BufferSource | WebAssembly.Module
}

export interface LoadedTermy {
  module: TermyWasmModule
  createTerminal(options?: CreateTermyTerminalOptions): TermyCore
  createRenderer(host: HTMLElement | null, options?: CreateTermyRendererOptions): TermyRenderer
  defaultRenderConfig(): TermyRenderConfig
  renderConfigFromContents(contents: string): TermyRenderConfig
}

export interface CreateTermyTerminalOptions {
  cols?: number
  rows?: number
  cellWidth?: number
  cellHeight?: number
  configContents?: string
  scrollback?: number
}

export interface XtermDisposable {
  dispose(): void
}

export interface XtermTerminalLike {
  cols: number
  rows: number
  options?: Record<string, unknown>
  write(data: string | Uint8Array, callback?: () => void): void
  onData?(listener: (data: string) => void): XtermDisposable
  onResize?(listener: (size: { cols: number; rows: number }) => void): XtermDisposable
}

export interface AttachTermyToXtermOptions extends CreateTermyTerminalOptions {
  termy?: LoadedTermy
  core?: TermyCore
  renderConfig?: TermyRenderConfig
  configContents?: string
  onInput?: (data: string) => void
  onResize?: (size: { cols: number; rows: number }) => void
  applyTheme?: boolean
  silenceDeprecation?: boolean
}

export interface TermyXtermBridge {
  core: TermyCore
  write(data: string | Uint8Array): TermyFeedResult
  resize(cols?: number, rows?: number): void
  snapshot(): TermyFrame
  search(query: string): TermySearchMatch[]
  dispose(): void
}

const textEncoder = new TextEncoder()

let attachTermyToXtermDeprecationWarned = false

export async function loadTermy(options: LoadTermyOptions = {}): Promise<LoadedTermy> {
  const moduleUrl = options.moduleUrl
  const usesBundledModule = moduleUrl === undefined
  const wasmModule =
    usesBundledModule
      ? (bundledWasmModule as TermyWasmModule)
      : ((await import(/* @vite-ignore */ moduleUrl.toString())) as TermyWasmModule)

  if (options.wasmUrl === undefined) {
    if (usesBundledModule) {
      await wasmModule.default({ module_or_path: embeddedWasmBytes() })
    } else {
      await wasmModule.default()
    }
  } else {
    await wasmModule.default({ module_or_path: options.wasmUrl })
  }

  const loaded: LoadedTermy = {
    module: wasmModule,
    createTerminal(createOptions = {}) {
      const cols = createOptions.cols ?? 80
      const rows = createOptions.rows ?? 24
      const cellWidth = createOptions.cellWidth ?? 9
      const cellHeight = createOptions.cellHeight ?? 18
      const terminal = wasmModule.TermyTerminal.withCellSize(cols, rows, cellWidth, cellHeight)

      if (createOptions.scrollback !== undefined) {
        terminal.setScrollback(createOptions.scrollback)
      }

      if (createOptions.configContents) {
        terminal.setConfigContents(createOptions.configContents)
      }

      return terminal
    },
    createRenderer(host, rendererOptions = {}) {
      return createTermyRenderer(host, { ...rendererOptions, termy: loaded })
    },
    defaultRenderConfig: wasmModule.defaultRenderConfig,
    renderConfigFromContents: wasmModule.renderConfigFromContents,
  }
  return loaded
}

/**
 * @deprecated `attachTermyToXterm` double-parses input — once via `xterm.write()` for
 * rendering and again via `core.feed()` for the libtermy snapshot/search index — which
 * wastes CPU and can desync the two parsers on edge-case escape sequences. Use the
 * first-party {@link createTermyRenderer} instead; it owns rendering, keyboard input,
 * selection, scrollback, and URL detection in a single pass. See the "Migration: from
 * xterm.js to first-party rendering" section of the libtermy.js README for a side-by-side
 * example. Pass `silenceDeprecation: true` to suppress the runtime warning.
 */
export function attachTermyToXterm(
  xterm: XtermTerminalLike,
  options: AttachTermyToXtermOptions = {},
): TermyXtermBridge {
  if (!options.silenceDeprecation && !attachTermyToXtermDeprecationWarned) {
    attachTermyToXtermDeprecationWarned = true
    console.warn(
      '[libtermy.js] attachTermyToXterm is deprecated and double-parses input. ' +
        'Migrate to createTermyRenderer (see README "Migration" section). ' +
        'Pass { silenceDeprecation: true } to hide this warning.',
    )
  }

  const loaded = options.termy
  const core =
    options.core ??
    loaded?.createTerminal({
      cols: options.cols ?? xterm.cols,
      rows: options.rows ?? xterm.rows,
      cellWidth: options.cellWidth,
      cellHeight: options.cellHeight,
      configContents: options.configContents,
    })

  if (!core) {
    throw new Error('attachTermyToXterm requires either options.termy or options.core')
  }

  const renderConfig =
    options.renderConfig ??
    (options.configContents && loaded?.renderConfigFromContents(options.configContents)) ??
    loaded?.defaultRenderConfig()

  if (options.applyTheme !== false && renderConfig) {
    applyRenderConfigToXterm(xterm, renderConfig)
  }

  const disposables: XtermDisposable[] = []

  if (xterm.onData && options.onInput) {
    disposables.push(xterm.onData(options.onInput))
  }

  if (xterm.onResize) {
    disposables.push(
      xterm.onResize((size) => {
        core.resize(size.cols, size.rows, options.cellWidth ?? 9, options.cellHeight ?? 18)
        options.onResize?.(size)
      }),
    )
  }

  return {
    core,
    write(data) {
      xterm.write(data)
      const result = core.feed(bytesFromXtermData(data))
      for (const response of result.responses) {
        options.onInput?.(response)
      }
      return result
    },
    resize(cols = xterm.cols, rows = xterm.rows) {
      core.resize(cols, rows, options.cellWidth ?? 9, options.cellHeight ?? 18)
    },
    snapshot() {
      return core.snapshot()
    },
    search(query) {
      return core.search(query)
    },
    dispose() {
      for (const disposable of disposables.splice(0)) {
        disposable.dispose()
      }
    },
  }
}

export function applyRenderConfigToXterm(
  xterm: XtermTerminalLike,
  config: TermyRenderConfig,
): void {
  xterm.options = {
    ...xterm.options,
    cursorBlink: config.cursorBlink,
    cursorStyle: config.cursorStyle === 'line' ? 'bar' : 'block',
    fontFamily: config.fontFamily,
    fontSize: config.fontSize,
    lineHeight: config.lineHeight,
    theme: toXtermTheme(config),
  }
}

export function toXtermTheme(config: TermyRenderConfig): Record<string, string> {
  const ansi = config.ansi.map(colorToHex)

  return {
    foreground: colorToHex(config.foreground),
    background: colorToHex(withOpacity(config.background, config.backgroundOpacity)),
    cursor: colorToHex(config.cursor),
    black: ansi[0],
    red: ansi[1],
    green: ansi[2],
    yellow: ansi[3],
    blue: ansi[4],
    magenta: ansi[5],
    cyan: ansi[6],
    white: ansi[7],
    brightBlack: ansi[8],
    brightRed: ansi[9],
    brightGreen: ansi[10],
    brightYellow: ansi[11],
    brightBlue: ansi[12],
    brightMagenta: ansi[13],
    brightCyan: ansi[14],
    brightWhite: ansi[15],
  }
}

export function colorToHex(color: TermyColor): string {
  const channel = (value: number) =>
    Math.max(0, Math.min(255, Math.round(value))).toString(16).padStart(2, '0')

  return `#${channel(color.r)}${channel(color.g)}${channel(color.b)}`
}

function withOpacity(color: TermyColor, opacity: number): TermyColor {
  if (opacity >= 1) {
    return color
  }

  const mix = (channel: number) => Math.round(channel * opacity)
  return {
    ...color,
    r: mix(color.r),
    g: mix(color.g),
    b: mix(color.b),
  }
}

function bytesFromXtermData(data: string | Uint8Array): Uint8Array {
  return typeof data === 'string' ? textEncoder.encode(data) : data
}

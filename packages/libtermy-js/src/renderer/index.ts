import type { LoadedTermy, TermyCore, TermyRenderConfig } from '../index'
import { createCanvas2dRenderer } from './backends/canvas2d'
import { createHeadlessRenderer, isHeadlessHost } from './backends/headless'
import { createWebgl2Renderer } from './backends/webgl2'
import type {
  CreateTermyRendererOptions,
  TermyRenderer,
  TermyRendererBackend,
} from './types'
import {
  createWorkerCoreBridge,
  type WorkerCoreBridge,
} from './worker/bridge'

export * from './types'
export {
  encodeKeystroke,
  DEFAULT_KEYBOARD_MODE,
  EMPTY_MODIFIERS,
} from './keyboard'
export type { Keystroke, KeyModifiers, TerminalKeyboardMode } from './keyboard'
export { serializeFrameToAnsi } from './serialize'
export { attachDomInput } from './dom-input'
export type { DomInputBindings, DomInputController } from './dom-input'

const DEFAULT_COLS = 80
const DEFAULT_ROWS = 24
const DEFAULT_CELL_WIDTH = 9
const DEFAULT_CELL_HEIGHT = 18

export function createTermyRenderer(
  host: HTMLElement | null,
  options: CreateTermyRendererOptions = {},
): TermyRenderer {
  const cols = options.cols ?? DEFAULT_COLS
  const rows = options.rows ?? DEFAULT_ROWS
  const cellWidth = options.cellWidth ?? DEFAULT_CELL_WIDTH
  const cellHeight = options.cellHeight ?? DEFAULT_CELL_HEIGHT

  let workerBridge: WorkerCoreBridge | null = null
  let core: TermyCore | null
  if (options.workerized) {
    workerBridge = createWorkerizedCore(options, cols, rows, cellWidth, cellHeight)
    core = workerBridge
  } else {
    core = resolveCore(options, host)
  }

  if (!core) {
    throw new Error(
      'createTermyRenderer requires either options.core or options.termy (call loadTermy() first)',
    )
  }

  const backend = resolveBackend(host, options)

  // In workerized mode the worker has already been told the initial geometry
  // and config via the `init` message; calling resize/setConfigContents again
  // here would round-trip needlessly. The non-worker path keeps the existing
  // synchronous behavior.
  let renderConfig: TermyRenderConfig
  if (workerBridge) {
    renderConfig = workerBridge.setConfigContents(options.configContents ?? '')
    if (options.renderConfig) {
      renderConfig = options.renderConfig
    }
    // Subscribe so we can prod a backend repaint when the worker pushes a new
    // snapshot. The canvas2d / webgl2 backends don't currently listen for
    // this event — see TODO below — but headless / advanced consumers can
    // use `core.subscribeToSnapshot` directly.
    workerBridge.subscribeToSnapshot(() => {
      if (host) {
        // TODO: canvas2d.ts and webgl2.ts should listen for `termy:snapshot`
        // on their host element and call their internal `scheduleRender()`.
        // For now the cursor-blink timer (500ms) provides a coarse repaint
        // loop, so workerized rendering still functions but with up to half
        // a second of snapshot lag for inputs that don't otherwise call
        // write/resize.
        host.dispatchEvent(new CustomEvent('termy:snapshot'))
      }
    })
  } else {
    core.resize(cols, rows, cellWidth, cellHeight)
    renderConfig = applyConfig(core, options)
  }

  if (backend === 'headless') {
    return createHeadlessRenderer({
      core,
      cols,
      rows,
      cellWidth,
      cellHeight,
      backend,
    })
  }

  if (backend === 'canvas2d' || backend === 'auto') {
    if (!host) {
      throw new Error(
        `Backend "${backend}" requires an HTMLElement host. Pass host=null with backend:'headless' for headless rendering.`,
      )
    }
    return createCanvas2dRenderer({
      host,
      core,
      renderConfig,
      options,
      initialCols: cols,
      initialRows: rows,
      initialCellWidth: cellWidth,
      initialCellHeight: cellHeight,
      backend: backend === 'auto' ? 'canvas2d' : backend,
    })
  }

  // TODO: implement a real WebGPU backend; for now both 'webgl' and 'webgpu'
  // construct the shared WebGL2 backend and fall back to canvas2d when the
  // context cannot be created (e.g. unsupported browser, blocked by policy).
  if (backend === 'webgl' || backend === 'webgpu') {
    if (!host) {
      throw new Error(
        `Backend "${backend}" requires an HTMLElement host. Pass host=null with backend:'headless' for headless rendering.`,
      )
    }
    const probeCanvas =
      typeof document !== 'undefined' ? document.createElement('canvas') : null
    const probeCtx = probeCanvas?.getContext('webgl2') ?? null
    if (!probeCtx) {
      if (typeof console !== 'undefined') {
        console.warn(
          `TermyRenderer backend "${backend}" is unavailable (no WebGL2 context); falling back to canvas2d.`,
        )
      }
      return createCanvas2dRenderer({
        host,
        core,
        renderConfig,
        options,
        initialCols: cols,
        initialRows: rows,
        initialCellWidth: cellWidth,
        initialCellHeight: cellHeight,
        backend: 'canvas2d',
      })
    }
    // Drop the probe context immediately so we don't leak a GL resource.
    const loseExt = probeCtx.getExtension('WEBGL_lose_context')
    loseExt?.loseContext()
    return createWebgl2Renderer({
      host,
      core,
      renderConfig,
      options,
      initialCols: cols,
      initialRows: rows,
      initialCellWidth: cellWidth,
      initialCellHeight: cellHeight,
      backend,
    })
  }

  throw new Error(
    `TermyRenderer backend "${backend}" is not implemented yet. ` +
      `Available backends: canvas2d, webgl, webgpu, headless.`,
  )
}

function applyConfig(
  core: TermyCore,
  options: CreateTermyRendererOptions,
): TermyRenderConfig {
  if (options.renderConfig) {
    return options.renderConfig
  }
  if (options.configContents) {
    return core.setConfigContents(options.configContents)
  }
  return core.setConfigContents('')
}

function resolveCore(
  options: CreateTermyRendererOptions,
  host: HTMLElement | null,
): TermyCore | null {
  if (options.core) {
    return options.core
  }
  if (options.termy) {
    return options.termy.createTerminal({
      cols: options.cols,
      rows: options.rows,
      cellWidth: options.cellWidth,
      cellHeight: options.cellHeight,
      configContents: options.configContents,
    })
  }
  void host
  return null
}

function resolveBackend(
  host: HTMLElement | null,
  options: CreateTermyRendererOptions,
): TermyRendererBackend {
  if (options.backend && options.backend !== 'auto') {
    return options.backend
  }
  if (isHeadlessHost(host, options)) {
    return 'headless'
  }
  return 'auto'
}

export function attachRendererToLoaded(loaded: LoadedTermy): LoadedTermy & {
  createRenderer: (host: HTMLElement | null, opts?: CreateTermyRendererOptions) => TermyRenderer
} {
  return {
    ...loaded,
    createRenderer(host, opts = {}) {
      return createTermyRenderer(host, { ...opts, termy: loaded })
    },
  }
}

function createWorkerizedCore(
  options: CreateTermyRendererOptions,
  cols: number,
  rows: number,
  cellWidth: number,
  cellHeight: number,
): WorkerCoreBridge {
  if (!options.termy) {
    // The bridge needs main-thread access to `renderConfigFromContents` so it
    // can answer `setConfigContents` synchronously. Asking the user to call
    // `loadTermy()` once on the main thread keeps the workerized contract
    // honest without forcing us to load a second WASM copy here.
    throw new Error(
      'createTermyRenderer({ workerized: true }) requires options.termy. ' +
        'Call await loadTermy() first and pass the result through.',
    )
  }
  return createWorkerCoreBridge({
    termy: options.termy,
    cols,
    rows,
    cellWidth,
    cellHeight,
    configContents: options.configContents,
    scrollback: options.scrollback,
  })
}

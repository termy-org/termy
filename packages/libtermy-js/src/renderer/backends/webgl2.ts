import type {
  TermyCell,
  TermyColor,
  TermyCore,
  TermyFeedResult,
  TermyFrame,
  TermyRenderConfig,
  TermySearchMatch,
} from '../../index'
import { BellHandler } from '../bell'
import {
  createLifecycleDispatchers,
  dispatchTermyEvents,
  type LifecycleEventDispatchers,
} from '../events'
import { attachDomInput, type DomInputController } from '../dom-input'
import { DEFAULT_KEYBOARD_MODE, type TerminalKeyboardMode } from '../keyboard'
import { detectLinks, findLinkAt, type DetectedLink } from '../links'
import { SearchHighlightTracker } from '../search-highlight'
import { SelectionTracker, type SelectionMode } from '../selection'
import { serializeFrameToAnsi } from '../serialize'
import type {
  CreateTermyRendererOptions,
  Disposable,
  LinkPayload,
  ResizePayload,
  SearchOptions,
  SelectionPayload,
  TermyRenderer,
  TermyRendererBackend,
} from '../types'

interface Webgl2RendererInit {
  host: HTMLElement
  core: TermyCore
  renderConfig: TermyRenderConfig
  options: CreateTermyRendererOptions
  initialCols: number
  initialRows: number
  initialCellWidth: number
  initialCellHeight: number
  backend: TermyRendererBackend
}

type Listener<T> = (value: T) => void

// Doubly-linked-list node so we can do O(1) LRU touch/evict/lookup-by-fit.
// `key` is the atlas key (`char|bold|renderText`), fg is intentionally NOT
// part of the key because glyphs are baked as white-alpha and colored per
// instance in the shader.
interface AtlasEntry {
  key: string
  u: number
  v: number
  w: number
  h: number
  prev: AtlasEntry | null
  next: AtlasEntry | null
}

const TEXT_ENCODER = new TextEncoder()
const DEFAULT_FPS = 60
const MULTI_CLICK_MS = 350
const ATLAS_SIZE = 2048
const ATLAS_ROW_PAD = 1
// Glyph instance: cellRect(vec4) + atlasRect(vec4) + fgColor(vec4) = 12 floats.
const GLYPH_FLOATS_PER_INSTANCE = 12
// Solid instance (bg + overlay): cellRect(vec4) + color(vec4) = 8 floats.
const SOLID_FLOATS_PER_INSTANCE = 8
// Cap on how many separate bufferSubData uploads we'll do per pass before
// falling back to a single full upload. Tuned empirically: 8 small uploads
// is still cheaper than a 100x40 = 4000-instance full upload at ~12 floats
// each.
const MAX_DIRTY_RUNS = 8

const VERTEX_SHADER_SRC = /* glsl */ `#version 300 es
in vec2 a_quadPos;
in vec4 a_cellRect;
in vec4 a_atlasRect;
in vec4 a_fgColor;
out vec2 v_uv;
out vec4 v_fgColor;
uniform vec2 u_resolution;
uniform vec2 u_atlasResolution;
void main() {
  vec2 worldPx = a_cellRect.xy + a_quadPos * a_cellRect.zw;
  vec2 clip = (worldPx / u_resolution) * 2.0 - 1.0;
  gl_Position = vec4(clip.x, -clip.y, 0.0, 1.0);
  v_uv = (a_atlasRect.xy + a_quadPos * a_atlasRect.zw) / u_atlasResolution;
  v_fgColor = a_fgColor;
}
`

// Atlas is baked as white text on transparent (premultiplied alpha). Sampling
// returns (a, a, a, a) for white pixels. We discard the rgb component and
// modulate by the per-instance v_fgColor, producing a premultiplied output
// matching the gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA blend mode used by the
// solid passes (so glyphs and backgrounds composite identically).
const FRAGMENT_SHADER_SRC = /* glsl */ `#version 300 es
precision mediump float;
in vec2 v_uv;
in vec4 v_fgColor;
out vec4 outColor;
uniform sampler2D u_atlas;
void main() {
  vec4 g = texture(u_atlas, v_uv);
  outColor = vec4(v_fgColor.rgb, g.a * v_fgColor.a);
}
`

const SOLID_VERTEX_SHADER_SRC = /* glsl */ `#version 300 es
in vec2 a_quadPos;
in vec4 a_cellRect;
in vec4 a_color;
out vec4 v_color;
uniform vec2 u_resolution;
void main() {
  vec2 worldPx = a_cellRect.xy + a_quadPos * a_cellRect.zw;
  vec2 clip = (worldPx / u_resolution) * 2.0 - 1.0;
  gl_Position = vec4(clip.x, -clip.y, 0.0, 1.0);
  v_color = a_color;
}
`

const SOLID_FRAGMENT_SHADER_SRC = /* glsl */ `#version 300 es
precision mediump float;
in vec4 v_color;
out vec4 outColor;
void main() {
  outColor = v_color;
}
`

interface AtlasStats {
  hits: number
  misses: number
  evictions: number
  resets: number
}

export function createWebgl2Renderer(init: Webgl2RendererInit): TermyRenderer {
  const inputListeners = new Set<Listener<Uint8Array>>()
  const resizeListeners = new Set<Listener<ResizePayload>>()
  const selectionListeners = new Set<Listener<SelectionPayload>>()
  const linkListeners = new Set<Listener<LinkPayload>>()

  const renderConfig = init.renderConfig
  let cols = init.initialCols
  let rows = init.initialRows
  let cellWidth = init.initialCellWidth
  let cellHeight = init.initialCellHeight
  let devicePixelRatio = readDpr()
  let lastSnapshot: TermyFrame | null = null
  let needsRender = true
  let cursorVisible = true
  let cursorBlinkTimer: ReturnType<typeof setInterval> | null = null
  let rafHandle: number | null = null
  let lastRenderTime = 0
  let disposed = false
  // True when the next frame should rebuild from scratch: resize, atlas reset,
  // scroll (displayOffset change), etc. Cleared after a successful full paint.
  let needsFullRepaint = true
  // True when only the overlay layer needs to redraw (cursor blink, selection
  // change, link hover toggle). Skips the bg + glyph passes entirely.
  let overlayDirty = false

  if (init.options.scrollback !== undefined) {
    init.core.setScrollback(init.options.scrollback)
  }

  const canvas = document.createElement('canvas')
  canvas.style.display = 'block'
  canvas.style.width = '100%'
  canvas.style.height = '100%'
  init.host.appendChild(canvas)

  const glOrNull = canvas.getContext('webgl2', {
    alpha: init.options.allowTransparency ?? true,
    premultipliedAlpha: true,
    antialias: false,
  })
  if (!glOrNull) {
    canvas.remove()
    throw new Error('webgl2 backend requires a WebGL2 context')
  }
  const gl: WebGL2RenderingContext = glOrNull

  // Offscreen 2D canvas for glyph rasterization, then uploaded to atlas texture.
  const glyphCanvas = document.createElement('canvas')
  const glyphCtxOrNull = glyphCanvas.getContext('2d')
  if (!glyphCtxOrNull) {
    canvas.remove()
    throw new Error('webgl2 backend requires a 2D context for glyph rasterization')
  }
  const glyphCtx: CanvasRenderingContext2D = glyphCtxOrNull

  // LRU is a doubly linked list with map index for O(1) access. `lruHead` is
  // least-recently-used (eviction candidate); `lruTail` is most-recently-used.
  const atlasEntries = new Map<string, AtlasEntry>()
  let lruHead: AtlasEntry | null = null
  let lruTail: AtlasEntry | null = null
  let atlasCursorX = 0
  let atlasCursorY = 0
  let atlasRowHeight = 0
  let atlasTexture: WebGLTexture | null = null
  let atlasNeedsReset = true

  // Internal-only counters for debugging. Not exposed.
  const atlasStats: AtlasStats = { hits: 0, misses: 0, evictions: 0, resets: 0 }
  void atlasStats

  // Programs and buffers.
  const glyphProgram = createProgram(gl, VERTEX_SHADER_SRC, FRAGMENT_SHADER_SRC)
  const solidProgram = createProgram(gl, SOLID_VERTEX_SHADER_SRC, SOLID_FRAGMENT_SHADER_SRC)

  const glyphLocs = {
    aQuadPos: gl.getAttribLocation(glyphProgram, 'a_quadPos'),
    aCellRect: gl.getAttribLocation(glyphProgram, 'a_cellRect'),
    aAtlasRect: gl.getAttribLocation(glyphProgram, 'a_atlasRect'),
    aFgColor: gl.getAttribLocation(glyphProgram, 'a_fgColor'),
    uResolution: gl.getUniformLocation(glyphProgram, 'u_resolution'),
    uAtlasResolution: gl.getUniformLocation(glyphProgram, 'u_atlasResolution'),
    uAtlas: gl.getUniformLocation(glyphProgram, 'u_atlas'),
  }
  const solidLocs = {
    aQuadPos: gl.getAttribLocation(solidProgram, 'a_quadPos'),
    aCellRect: gl.getAttribLocation(solidProgram, 'a_cellRect'),
    aColor: gl.getAttribLocation(solidProgram, 'a_color'),
    uResolution: gl.getUniformLocation(solidProgram, 'u_resolution'),
  }

  const quadBuffer = gl.createBuffer()
  if (!quadBuffer) {
    canvas.remove()
    throw new Error('webgl2 backend could not allocate quad buffer')
  }
  gl.bindBuffer(gl.ARRAY_BUFFER, quadBuffer)
  gl.bufferData(
    gl.ARRAY_BUFFER,
    new Float32Array([0, 0, 1, 0, 0, 1, 0, 1, 1, 0, 1, 1]),
    gl.STATIC_DRAW,
  )

  const glyphInstanceBuffer = gl.createBuffer()
  const bgInstanceBuffer = gl.createBuffer()
  const overlayInstanceBuffer = gl.createBuffer()
  const glyphVao = gl.createVertexArray()
  const bgVao = gl.createVertexArray()
  const overlayVao = gl.createVertexArray()
  if (
    !glyphInstanceBuffer ||
    !bgInstanceBuffer ||
    !overlayInstanceBuffer ||
    !glyphVao ||
    !bgVao ||
    !overlayVao
  ) {
    canvas.remove()
    throw new Error('webgl2 backend could not allocate buffers/VAOs')
  }

  // Glyph VAO: quad + per-instance cellRect (vec4) + atlasRect (vec4) + fgColor (vec4)
  // = 12 floats stride (48 bytes).
  gl.bindVertexArray(glyphVao)
  gl.bindBuffer(gl.ARRAY_BUFFER, quadBuffer)
  gl.enableVertexAttribArray(glyphLocs.aQuadPos)
  gl.vertexAttribPointer(glyphLocs.aQuadPos, 2, gl.FLOAT, false, 0, 0)
  gl.bindBuffer(gl.ARRAY_BUFFER, glyphInstanceBuffer)
  const glyphStride = GLYPH_FLOATS_PER_INSTANCE * 4
  gl.enableVertexAttribArray(glyphLocs.aCellRect)
  gl.vertexAttribPointer(glyphLocs.aCellRect, 4, gl.FLOAT, false, glyphStride, 0)
  gl.vertexAttribDivisor(glyphLocs.aCellRect, 1)
  gl.enableVertexAttribArray(glyphLocs.aAtlasRect)
  gl.vertexAttribPointer(glyphLocs.aAtlasRect, 4, gl.FLOAT, false, glyphStride, 4 * 4)
  gl.vertexAttribDivisor(glyphLocs.aAtlasRect, 1)
  gl.enableVertexAttribArray(glyphLocs.aFgColor)
  gl.vertexAttribPointer(glyphLocs.aFgColor, 4, gl.FLOAT, false, glyphStride, 8 * 4)
  gl.vertexAttribDivisor(glyphLocs.aFgColor, 1)
  gl.bindVertexArray(null)

  // Background VAO: quad + per-instance cellRect (vec4) + color (vec4) = 8 floats stride.
  const solidStride = SOLID_FLOATS_PER_INSTANCE * 4
  gl.bindVertexArray(bgVao)
  gl.bindBuffer(gl.ARRAY_BUFFER, quadBuffer)
  gl.enableVertexAttribArray(solidLocs.aQuadPos)
  gl.vertexAttribPointer(solidLocs.aQuadPos, 2, gl.FLOAT, false, 0, 0)
  gl.bindBuffer(gl.ARRAY_BUFFER, bgInstanceBuffer)
  gl.enableVertexAttribArray(solidLocs.aCellRect)
  gl.vertexAttribPointer(solidLocs.aCellRect, 4, gl.FLOAT, false, solidStride, 0)
  gl.vertexAttribDivisor(solidLocs.aCellRect, 1)
  gl.enableVertexAttribArray(solidLocs.aColor)
  gl.vertexAttribPointer(solidLocs.aColor, 4, gl.FLOAT, false, solidStride, 4 * 4)
  gl.vertexAttribDivisor(solidLocs.aColor, 1)
  gl.bindVertexArray(null)

  // Overlay VAO: same layout as background; used for cursor, selection, link underline.
  gl.bindVertexArray(overlayVao)
  gl.bindBuffer(gl.ARRAY_BUFFER, quadBuffer)
  gl.enableVertexAttribArray(solidLocs.aQuadPos)
  gl.vertexAttribPointer(solidLocs.aQuadPos, 2, gl.FLOAT, false, 0, 0)
  gl.bindBuffer(gl.ARRAY_BUFFER, overlayInstanceBuffer)
  gl.enableVertexAttribArray(solidLocs.aCellRect)
  gl.vertexAttribPointer(solidLocs.aCellRect, 4, gl.FLOAT, false, solidStride, 0)
  gl.vertexAttribDivisor(solidLocs.aCellRect, 1)
  gl.enableVertexAttribArray(solidLocs.aColor)
  gl.vertexAttribPointer(solidLocs.aColor, 4, gl.FLOAT, false, solidStride, 4 * 4)
  gl.vertexAttribDivisor(solidLocs.aColor, 1)
  gl.bindVertexArray(null)

  gl.enable(gl.BLEND)
  gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA)

  const selection = new SelectionTracker(init.options.wordSeparator)
  const searchHighlight = new SearchHighlightTracker()
  const bell = new BellHandler({ mode: init.options.bellSound ?? 'none' })
  const lifecycleDispatchers: LifecycleEventDispatchers = createLifecycleDispatchers()
  lifecycleDispatchers.bell.add(() => bell.trigger())
  const keyboardMode: TerminalKeyboardMode = { ...DEFAULT_KEYBOARD_MODE }
  let detectedLinks: DetectedLink[] = []
  let hoveredLink: DetectedLink | null = null
  let lastClickTime = 0
  let lastClickCount = 0
  let dragging = false

  // Persistent instance buffers sized to cols*rows. We slot each cell at a
  // fixed offset (row * cols + col) and zero-out cells with no glyph/no bg.
  // This means we can `bufferSubData` only the dirty range without having
  // to compact instances each frame.
  let glyphSlotData = new Float32Array(0)
  let bgSlotData = new Float32Array(0)
  // Per-slot flag: whether the slot has a non-zero glyph/bg. Used to keep the
  // GPU's `drawArraysInstanced` count correct. We allocate the buffer once at
  // `cols*rows * stride` capacity and always draw all `cols*rows` instances;
  // empty slots have zero rect size so they contribute no fragments. That's
  // cheaper than re-uploading a compacted array every frame.
  let slotCapacity = 0
  let overlayInstanceData = new Float32Array(0)

  function readDpr(): number {
    return typeof window !== 'undefined' ? window.devicePixelRatio || 1 : 1
  }

  function fontString(): string {
    const family = init.options.fontFamily ?? renderConfig.fontFamily
    const size = init.options.fontSize ?? renderConfig.fontSize
    return `${size}px ${family}`
  }

  // Standardized CSS font shorthand for a given style combo. Italic always
  // precedes bold per CSS grammar.
  function fontStringFor(bold: boolean, italic: boolean): string {
    const family = init.options.fontFamily ?? renderConfig.fontFamily
    const size = init.options.fontSize ?? renderConfig.fontSize
    const style = italic ? 'italic ' : ''
    const weight = bold ? 'bold ' : ''
    return `${style}${weight}${size}px ${family}`
  }

  // Resolve effective fg/bg after applying SGR reverse. Mirrors canvas2d so
  // both backends produce identical pixels for the same cell.
  function effectiveColors(cell: TermyCell): {
    fg: TermyColor
    bg: TermyColor
    bgIsDefault: boolean
  } {
    if (!cell.reverse) {
      return { fg: cell.fg, bg: cell.bg, bgIsDefault: cell.usesTerminalDefaultBg }
    }
    const effFg = cell.usesTerminalDefaultBg ? renderConfig.background : cell.bg
    return { fg: effFg, bg: cell.fg, bgIsDefault: false }
  }

  function measureCell(): { width: number; height: number } {
    glyphCtx.font = fontString()
    const metrics = glyphCtx.measureText('M')
    const width = metrics.width || cellWidth
    const lineHeight = init.options.lineHeight ?? renderConfig.lineHeight
    const baseSize = init.options.fontSize ?? renderConfig.fontSize
    const height = Math.max(1, Math.round(baseSize * lineHeight))
    return { width, height }
  }

  function lruRemove(entry: AtlasEntry): void {
    if (entry.prev) entry.prev.next = entry.next
    else lruHead = entry.next
    if (entry.next) entry.next.prev = entry.prev
    else lruTail = entry.prev
    entry.prev = null
    entry.next = null
  }

  function lruPushTail(entry: AtlasEntry): void {
    entry.prev = lruTail
    entry.next = null
    if (lruTail) lruTail.next = entry
    else lruHead = entry
    lruTail = entry
  }

  function lruTouch(entry: AtlasEntry): void {
    if (lruTail === entry) return
    lruRemove(entry)
    lruPushTail(entry)
  }

  function resetAtlas(): void {
    atlasEntries.clear()
    lruHead = null
    lruTail = null
    atlasCursorX = 0
    atlasCursorY = 0
    atlasRowHeight = 0
    atlasNeedsReset = true
    atlasStats.resets++
  }

  function ensureAtlasTexture(): WebGLTexture {
    if (atlasTexture && !atlasNeedsReset) return atlasTexture
    if (!atlasTexture) {
      atlasTexture = gl.createTexture()
      if (!atlasTexture) {
        throw new Error('webgl2 backend could not allocate atlas texture')
      }
    }
    gl.bindTexture(gl.TEXTURE_2D, atlasTexture)
    gl.texImage2D(
      gl.TEXTURE_2D,
      0,
      gl.RGBA,
      ATLAS_SIZE,
      ATLAS_SIZE,
      0,
      gl.RGBA,
      gl.UNSIGNED_BYTE,
      null,
    )
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR)
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR)
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE)
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE)
    atlasNeedsReset = false
    return atlasTexture
  }

  function ensureSlotCapacity(): void {
    const needed = cols * rows
    if (needed === slotCapacity) return
    slotCapacity = needed
    glyphSlotData = new Float32Array(needed * GLYPH_FLOATS_PER_INSTANCE)
    bgSlotData = new Float32Array(needed * SOLID_FLOATS_PER_INSTANCE)
    // Resize GPU buffers to match. `bufferData` with the typed array reserves
    // the capacity in one shot. We mark the contents dirty so the next paint
    // does a full upload.
    gl.bindBuffer(gl.ARRAY_BUFFER, glyphInstanceBuffer)
    gl.bufferData(gl.ARRAY_BUFFER, glyphSlotData, gl.DYNAMIC_DRAW)
    gl.bindBuffer(gl.ARRAY_BUFFER, bgInstanceBuffer)
    gl.bufferData(gl.ARRAY_BUFFER, bgSlotData, gl.DYNAMIC_DRAW)
    needsFullRepaint = true
  }

  function recomputeGrid(): void {
    devicePixelRatio = readDpr()
    const measured = measureCell()
    cellWidth = measured.width
    cellHeight = measured.height
    const paddingX = renderConfig.paddingX
    const paddingY = renderConfig.paddingY
    const hostRect = init.host.getBoundingClientRect()
    const usableWidth = Math.max(0, hostRect.width - paddingX * 2)
    const usableHeight = Math.max(0, hostRect.height - paddingY * 2)
    const nextCols = Math.max(1, Math.floor(usableWidth / cellWidth))
    const nextRows = Math.max(1, Math.floor(usableHeight / cellHeight))

    if (nextCols !== cols || nextRows !== rows) {
      cols = nextCols
      rows = nextRows
      init.core.resize(cols, rows, cellWidth, cellHeight)
      for (const listener of resizeListeners) {
        listener({ cols, rows })
      }
    }

    canvas.width = Math.max(1, Math.round(hostRect.width * devicePixelRatio))
    canvas.height = Math.max(1, Math.round(hostRect.height * devicePixelRatio))
    canvas.style.width = `${hostRect.width}px`
    canvas.style.height = `${hostRect.height}px`
    gl.viewport(0, 0, canvas.width, canvas.height)

    // Font/cell metrics changed: existing atlas glyphs are stale.
    resetAtlas()
    lastSnapshot = null
    needsFullRepaint = true
    needsRender = true
    ensureSlotCapacity()
  }

  // Glyph key now omits fg color: glyphs are baked white-alpha and tinted
  // per instance in the fragment shader. `width` is part of the key because
  // wide glyphs (width=2) are rasterized onto a 2*cellWidth atlas slot.
  //
  // Italic, underline, and strikethrough are baked into the rasterized glyph
  // (they affect the shape painted into the atlas) so they're part of the
  // key. Dim, reverse, and invisible are NOT — they only affect per-instance
  // fg/alpha, which the shader handles. Blink is a no-op for v1.
  function glyphKey(cell: TermyCell): string {
    return `${cell.char}|${cell.bold ? 1 : 0}|${cell.italic ? 1 : 0}|${cell.underline ? 1 : 0}|${cell.strikethrough ? 1 : 0}|${cell.renderText ? 1 : 0}|${cell.width}`
  }

  function rasterizeGlyphToImageData(cell: TermyCell, pxWidth: number, pxHeight: number): ImageData {
    glyphCanvas.width = pxWidth
    glyphCanvas.height = pxHeight
    const gctx = glyphCtx
    gctx.setTransform(1, 0, 0, 1, 0, 0)
    gctx.clearRect(0, 0, pxWidth, pxHeight)
    gctx.setTransform(devicePixelRatio, 0, 0, devicePixelRatio, 0, 0)

    const colSpan = cell.width === 2 ? 2 : 1
    const cellPxWidth = cellWidth * colSpan

    if (cell.renderText && cell.char.trim().length > 0) {
      gctx.font = fontStringFor(cell.bold, cell.italic)
      // Bake glyph as white. Alpha carries the coverage; the fragment shader
      // multiplies by the per-instance fg color. `UNPACK_PREMULTIPLY_ALPHA_WEBGL`
      // is set true at upload, so the texel becomes (a, a, a, a) for white.
      gctx.fillStyle = '#ffffff'
      gctx.textBaseline = 'top'
      const baseSize = init.options.fontSize ?? renderConfig.fontSize
      const yOffset = (cellHeight - baseSize) / 2
      gctx.fillText(cell.char, 0, yOffset)
    }

    // Bake decorations (underline / strikethrough) into the atlas glyph too
    // so a single drawArraysInstanced pass paints them along with the glyph.
    if (cell.underline) {
      gctx.fillStyle = '#ffffff'
      gctx.fillRect(0, cellHeight - 1, cellPxWidth, 1)
    }
    if (cell.strikethrough) {
      gctx.fillStyle = '#ffffff'
      gctx.fillRect(0, Math.floor(cellHeight / 2), cellPxWidth, 1)
    }

    gctx.setTransform(1, 0, 0, 1, 0, 0)
    return gctx.getImageData(0, 0, pxWidth, pxHeight)
  }

  // Try to find an LRU entry whose rect can fit a new glyph (i.e., entry has
  // identical dimensions, since all cells are uniform-sized). Walk from head
  // (least-recent) forward; any entry with matching w/h works.
  function findEvictionVictim(w: number, h: number): AtlasEntry | null {
    let cur = lruHead
    while (cur) {
      if (cur.w === w && cur.h === h) return cur
      cur = cur.next
    }
    return null
  }

  function allocateAtlasEntry(key: string, w: number, h: number): AtlasEntry | null {
    if (w > ATLAS_SIZE || h > ATLAS_SIZE) return null

    // Try strip-pack first (fast path while there's room).
    if (atlasCursorX + w > ATLAS_SIZE) {
      atlasCursorX = 0
      atlasCursorY += atlasRowHeight + ATLAS_ROW_PAD
      atlasRowHeight = 0
    }
    if (atlasCursorY + h <= ATLAS_SIZE) {
      const entry: AtlasEntry = {
        key,
        u: atlasCursorX,
        v: atlasCursorY,
        w,
        h,
        prev: null,
        next: null,
      }
      atlasCursorX += w + ATLAS_ROW_PAD
      if (h > atlasRowHeight) atlasRowHeight = h
      return entry
    }

    // Atlas full: try LRU eviction with a matching-size victim. All glyphs
    // share the same cell dimensions, so a matching-size victim is the
    // common case.
    const victim = findEvictionVictim(w, h)
    if (victim) {
      atlasEntries.delete(victim.key)
      lruRemove(victim)
      atlasStats.evictions++
      const entry: AtlasEntry = {
        key,
        u: victim.u,
        v: victim.v,
        w,
        h,
        prev: null,
        next: null,
      }
      return entry
    }

    // Last-resort: nuke the atlas. Forces a full repaint to re-bake glyphs.
    resetAtlas()
    needsFullRepaint = true
    if (atlasCursorY + h > ATLAS_SIZE) return null
    const entry: AtlasEntry = {
      key,
      u: atlasCursorX,
      v: atlasCursorY,
      w,
      h,
      prev: null,
      next: null,
    }
    atlasCursorX += w + ATLAS_ROW_PAD
    if (h > atlasRowHeight) atlasRowHeight = h
    return entry
  }

  function getOrCreateAtlasEntry(cell: TermyCell): AtlasEntry | null {
    const key = glyphKey(cell)
    const existing = atlasEntries.get(key)
    if (existing) {
      atlasStats.hits++
      lruTouch(existing)
      return existing
    }
    atlasStats.misses++
    // Wide glyphs (CJK/emoji/fullwidth) need a 2*cellWidth atlas slot so the
    // rasterized bitmap matches its on-screen footprint.
    const colSpan = cell.width === 2 ? 2 : 1
    const pxWidth = Math.max(1, Math.ceil(cellWidth * colSpan * devicePixelRatio))
    const pxHeight = Math.max(1, Math.ceil(cellHeight * devicePixelRatio))
    const entry = allocateAtlasEntry(key, pxWidth, pxHeight)
    if (!entry) return null

    const imageData = rasterizeGlyphToImageData(cell, pxWidth, pxHeight)
    ensureAtlasTexture()
    gl.bindTexture(gl.TEXTURE_2D, atlasTexture)
    gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, true)
    gl.texSubImage2D(
      gl.TEXTURE_2D,
      0,
      entry.u,
      entry.v,
      gl.RGBA,
      gl.UNSIGNED_BYTE,
      imageData,
    )

    atlasEntries.set(key, entry)
    lruPushTail(entry)
    return entry
  }

  function colorToFloat4(c: TermyColor, opacity = 1): [number, number, number, number] {
    return [c.r / 255, c.g / 255, c.b / 255, (c.a / 255) * opacity]
  }

  function bgClearColor(): [number, number, number, number] {
    return colorToFloat4(renderConfig.background, renderConfig.backgroundOpacity)
  }

  function cellsEqual(a: TermyCell, b: TermyCell): boolean {
    return (
      a.char === b.char &&
      a.bold === b.bold &&
      a.italic === b.italic &&
      a.underline === b.underline &&
      a.strikethrough === b.strikethrough &&
      a.dim === b.dim &&
      a.reverse === b.reverse &&
      a.blink === b.blink &&
      a.invisible === b.invisible &&
      a.renderText === b.renderText &&
      a.width === b.width &&
      a.usesTerminalDefaultBg === b.usesTerminalDefaultBg &&
      a.fg.r === b.fg.r &&
      a.fg.g === b.fg.g &&
      a.fg.b === b.fg.b &&
      a.fg.a === b.fg.a &&
      a.bg.r === b.bg.r &&
      a.bg.g === b.bg.g &&
      a.bg.b === b.bg.b &&
      a.bg.a === b.bg.a
    )
  }

  // Write a single cell's bg + glyph data into the persistent slot buffers.
  // Cells with no visible content write zero rects (drawArraysInstanced still
  // dispatches them but they're degenerate and skipped early in the pipeline).
  function writeSlot(frame: TermyFrame, idx: number): boolean {
    const cell = frame.cells[idx]
    const bgOff = idx * SOLID_FLOATS_PER_INSTANCE
    const gOff = idx * GLYPH_FLOATS_PER_INSTANCE
    if (!cell) {
      // Zero both slots.
      bgSlotData[bgOff + 0] = 0
      bgSlotData[bgOff + 1] = 0
      bgSlotData[bgOff + 2] = 0
      bgSlotData[bgOff + 3] = 0
      bgSlotData[bgOff + 4] = 0
      bgSlotData[bgOff + 5] = 0
      bgSlotData[bgOff + 6] = 0
      bgSlotData[bgOff + 7] = 0
      glyphSlotData[gOff + 0] = 0
      glyphSlotData[gOff + 1] = 0
      glyphSlotData[gOff + 2] = 0
      glyphSlotData[gOff + 3] = 0
      glyphSlotData[gOff + 4] = 0
      glyphSlotData[gOff + 5] = 0
      glyphSlotData[gOff + 6] = 0
      glyphSlotData[gOff + 7] = 0
      glyphSlotData[gOff + 8] = 0
      glyphSlotData[gOff + 9] = 0
      glyphSlotData[gOff + 10] = 0
      glyphSlotData[gOff + 11] = 0
      return true
    }

    // Right half of a wide glyph: emit a fully degenerate slot. The wide cell
    // one column to the left is responsible for painting the bg + glyph
    // across both columns.
    if (cell.width === 0) {
      bgSlotData[bgOff + 0] = 0
      bgSlotData[bgOff + 1] = 0
      bgSlotData[bgOff + 2] = 0
      bgSlotData[bgOff + 3] = 0
      bgSlotData[bgOff + 4] = 0
      bgSlotData[bgOff + 5] = 0
      bgSlotData[bgOff + 6] = 0
      bgSlotData[bgOff + 7] = 0
      glyphSlotData[gOff + 0] = 0
      glyphSlotData[gOff + 1] = 0
      glyphSlotData[gOff + 2] = 0
      glyphSlotData[gOff + 3] = 0
      glyphSlotData[gOff + 4] = 0
      glyphSlotData[gOff + 5] = 0
      glyphSlotData[gOff + 6] = 0
      glyphSlotData[gOff + 7] = 0
      glyphSlotData[gOff + 8] = 0
      glyphSlotData[gOff + 9] = 0
      glyphSlotData[gOff + 10] = 0
      glyphSlotData[gOff + 11] = 0
      return true
    }

    const col = idx % frame.cols
    const row = (idx - col) / frame.cols

    const paddingX = renderConfig.paddingX * devicePixelRatio
    const paddingY = renderConfig.paddingY * devicePixelRatio
    const cellWPx = cellWidth * devicePixelRatio
    const cellHPx = cellHeight * devicePixelRatio
    const colSpan = cell.width === 2 ? 2 : 1
    const cellWRect = cellWPx * colSpan
    const x = paddingX + col * cellWPx
    const y = paddingY + row * cellHPx

    const eff = effectiveColors(cell)

    // Background.
    if (!eff.bgIsDefault) {
      const [r, g, b, a] = colorToFloat4(eff.bg)
      bgSlotData[bgOff + 0] = x
      bgSlotData[bgOff + 1] = y
      bgSlotData[bgOff + 2] = cellWRect
      bgSlotData[bgOff + 3] = cellHPx
      bgSlotData[bgOff + 4] = r
      bgSlotData[bgOff + 5] = g
      bgSlotData[bgOff + 6] = b
      bgSlotData[bgOff + 7] = a
    } else {
      // Degenerate rect (width=0) so it's skipped.
      bgSlotData[bgOff + 0] = 0
      bgSlotData[bgOff + 1] = 0
      bgSlotData[bgOff + 2] = 0
      bgSlotData[bgOff + 3] = 0
      bgSlotData[bgOff + 4] = 0
      bgSlotData[bgOff + 5] = 0
      bgSlotData[bgOff + 6] = 0
      bgSlotData[bgOff + 7] = 0
    }

    // Glyph. `invisible` keeps the bg fill above but skips emitting glyph,
    // underline, and strikethrough. The atlas key for underline/strikethrough
    // already accounts for them being baked into the bitmap, so we just skip
    // the instance.
    const hasDecorations = cell.underline || cell.strikethrough
    const drawGlyph =
      !cell.invisible &&
      ((cell.renderText && cell.char.length > 0 && cell.char !== ' ') || hasDecorations)
    if (drawGlyph) {
      const entry = getOrCreateAtlasEntry(cell)
      if (entry) {
        // Dim: scale the fg alpha by 60% so the glyph (and any baked-in
        // decorations) render fainter.
        const alphaMul = cell.dim ? 0.6 : 1
        const [fr, fg, fb, fa] = colorToFloat4(eff.fg, alphaMul)
        glyphSlotData[gOff + 0] = x
        glyphSlotData[gOff + 1] = y
        glyphSlotData[gOff + 2] = cellWRect
        glyphSlotData[gOff + 3] = cellHPx
        glyphSlotData[gOff + 4] = entry.u
        glyphSlotData[gOff + 5] = entry.v
        glyphSlotData[gOff + 6] = entry.w
        glyphSlotData[gOff + 7] = entry.h
        glyphSlotData[gOff + 8] = fr
        glyphSlotData[gOff + 9] = fg
        glyphSlotData[gOff + 10] = fb
        glyphSlotData[gOff + 11] = fa
        return true
      }
      // Allocation failed (atlas truly full + couldn't evict): signal full
      // repaint by returning false. The caller will fall back.
      return false
    }
    glyphSlotData[gOff + 0] = 0
    glyphSlotData[gOff + 1] = 0
    glyphSlotData[gOff + 2] = 0
    glyphSlotData[gOff + 3] = 0
    glyphSlotData[gOff + 4] = 0
    glyphSlotData[gOff + 5] = 0
    glyphSlotData[gOff + 6] = 0
    glyphSlotData[gOff + 7] = 0
    glyphSlotData[gOff + 8] = 0
    glyphSlotData[gOff + 9] = 0
    glyphSlotData[gOff + 10] = 0
    glyphSlotData[gOff + 11] = 0
    return true
  }

  // Returns the list of dirty index runs. Each run is [startIdx, endIdxInclusive].
  // If non-contiguous runs exceed MAX_DIRTY_RUNS, returns a single [0, total-1]
  // run, signalling "just do a full upload".
  function computeDirtyRuns(frame: TermyFrame): Array<[number, number]> | null {
    if (!lastSnapshot) return null
    if (lastSnapshot.cols !== frame.cols || lastSnapshot.rows !== frame.rows) return null
    if (lastSnapshot.displayOffset !== frame.displayOffset) return null

    const total = frame.cols * frame.rows
    const runs: Array<[number, number]> = []
    let runStart = -1
    for (let i = 0; i < total; i++) {
      const a = lastSnapshot.cells[i]
      const b = frame.cells[i]
      let dirty = false
      if (!a || !b) dirty = a !== b
      else dirty = !cellsEqual(a, b)
      if (dirty) {
        if (runStart < 0) runStart = i
      } else if (runStart >= 0) {
        runs.push([runStart, i - 1])
        runStart = -1
        if (runs.length > MAX_DIRTY_RUNS) return [[0, total - 1]]
      }
    }
    if (runStart >= 0) runs.push([runStart, total - 1])
    return runs
  }

  function buildOverlayData(frame: TermyFrame): void {
    const paddingX = renderConfig.paddingX * devicePixelRatio
    const paddingY = renderConfig.paddingY * devicePixelRatio
    const cellWPx = cellWidth * devicePixelRatio
    const cellHPx = cellHeight * devicePixelRatio

    // Estimate: cursor + per-row selection bars + hover link underline +
    // always-on OSC8 underlines + search matches + bell flash overlay.
    const matches = searchHighlight.getMatches()
    const osc8Count = detectedLinks.reduce(
      (acc, link) => acc + (link.source === 'osc8' ? 1 : 0),
      0,
    )
    const maxOverlays = frame.rows + 3 + matches.length * 2 + osc8Count
    const overlay = new Float32Array(maxOverlays * 8)
    let count = 0

    // Search highlights (painted under selection so the selection still
    // takes visual precedence). Active match gets a slightly darker tint.
    if (searchHighlight.isActive()) {
      const activeIdx = searchHighlight.getActiveIndex()
      for (let i = 0; i < matches.length; i++) {
        const m = matches[i]
        if (!m) continue
        if (m.row < 0 || m.row >= frame.rows) continue
        const isActive = i === activeIdx
        const x = paddingX + m.startCol * cellWPx
        const y = paddingY + m.row * cellHPx
        const w = (m.endCol - m.startCol + 1) * cellWPx
        const off = count * 8
        overlay[off + 0] = x
        overlay[off + 1] = y
        overlay[off + 2] = w
        overlay[off + 3] = cellHPx
        overlay[off + 4] = 255 / 255
        overlay[off + 5] = 214 / 255
        overlay[off + 6] = 102 / 255
        overlay[off + 7] = isActive ? 0.55 : 0.35
        count++
      }
    }

    // Selection (alpha-blended).
    if (selection.isActive()) {
      const range = selection.getRange(frame)
      if (range) {
        const selColor: [number, number, number, number] = [120 / 255, 150 / 255, 255 / 255, 0.35]
        for (let row = range.startRow; row <= range.endRow; row++) {
          const fromCol = row === range.startRow ? range.startCol : 0
          const toCol = row === range.endRow ? range.endCol : frame.cols - 1
          const x = paddingX + fromCol * cellWPx
          const y = paddingY + row * cellHPx
          const w = (toCol - fromCol + 1) * cellWPx
          const off = count * 8
          overlay[off + 0] = x
          overlay[off + 1] = y
          overlay[off + 2] = w
          overlay[off + 3] = cellHPx
          overlay[off + 4] = selColor[0]
          overlay[off + 5] = selColor[1]
          overlay[off + 6] = selColor[2]
          overlay[off + 7] = selColor[3]
          count++
        }
      }
    }

    // Always-on OSC8 hyperlink underlines. These follow iTerm2/kitty/wezterm
    // convention: any cell that's part of an OSC8 hyperlink is underlined
    // regardless of hover state. (Regex-detected links remain hover-only and
    // are painted by the next block.)
    const underlineH = Math.max(1, devicePixelRatio)
    const [ulr, ulg, ulb, ula] = colorToFloat4(renderConfig.foreground)
    for (const link of detectedLinks) {
      if (link.source !== 'osc8') continue
      const x = paddingX + link.startCol * cellWPx
      const y = paddingY + (link.row + 1) * cellHPx - underlineH
      const w = (link.endCol - link.startCol + 1) * cellWPx
      const off = count * 8
      overlay[off + 0] = x
      overlay[off + 1] = y
      overlay[off + 2] = w
      overlay[off + 3] = underlineH
      overlay[off + 4] = ulr
      overlay[off + 5] = ulg
      overlay[off + 6] = ulb
      overlay[off + 7] = ula
      count++
    }

    // Hover underline for regex-detected URLs. OSC8 links are covered by the
    // always-on pass above; treat a legacy `undefined` source as regex.
    if (hoveredLink && (hoveredLink.source ?? 'regex') === 'regex') {
      const link = hoveredLink
      const x = paddingX + link.startCol * cellWPx
      const y = paddingY + (link.row + 1) * cellHPx - underlineH
      const w = (link.endCol - link.startCol + 1) * cellWPx
      const off = count * 8
      overlay[off + 0] = x
      overlay[off + 1] = y
      overlay[off + 2] = w
      overlay[off + 3] = underlineH
      overlay[off + 4] = ulr
      overlay[off + 5] = ulg
      overlay[off + 6] = ulb
      overlay[off + 7] = ula
      count++
    }

    // Cursor.
    if (frame.cursor && cursorVisible) {
      const cur = frame.cursor
      const x = paddingX + cur.col * cellWPx
      const y = paddingY + cur.row * cellHPx
      const [r, g, b, a] = colorToFloat4(renderConfig.cursor)
      const w = cur.style === 'line' ? Math.max(1, cellWPx / 8) : cellWPx
      const off = count * 8
      overlay[off + 0] = x
      overlay[off + 1] = y
      overlay[off + 2] = w
      overlay[off + 3] = cellHPx
      overlay[off + 4] = r
      overlay[off + 5] = g
      overlay[off + 6] = b
      overlay[off + 7] = a
      count++
    }

    // Bell visual flash. Paint last so it composites on top of everything
    // else. Intensity decays from 1 -> 0 over FLASH_DURATION_MS in `bell.ts`.
    {
      const now = typeof performance !== 'undefined' ? performance.now() : Date.now()
      const intensity = bell.visualIntensity(now)
      if (intensity > 0) {
        const off = count * 8
        // Account for the canvas dpr — the overlay rects are in device-pixel
        // coordinates, matching cellWPx/cellHPx above.
        const fg = renderConfig.foreground
        overlay[off + 0] = 0
        overlay[off + 1] = 0
        overlay[off + 2] = canvas.width
        overlay[off + 3] = canvas.height
        overlay[off + 4] = fg.r / 255
        overlay[off + 5] = fg.g / 255
        overlay[off + 6] = fg.b / 255
        overlay[off + 7] = intensity * (fg.a / 255)
        count++
      }
    }

    overlayInstanceData = overlay.subarray(0, count * 8)
  }

  function drawBgPass(frame: TermyFrame): void {
    const total = frame.cols * frame.rows
    if (total <= 0) return
    gl.useProgram(solidProgram)
    gl.uniform2f(solidLocs.uResolution, canvas.width, canvas.height)
    gl.bindVertexArray(bgVao)
    gl.bindBuffer(gl.ARRAY_BUFFER, bgInstanceBuffer)
    gl.drawArraysInstanced(gl.TRIANGLES, 0, 6, total)
    gl.bindVertexArray(null)
  }

  function drawGlyphPass(frame: TermyFrame): void {
    if (!atlasTexture) return
    const total = frame.cols * frame.rows
    if (total <= 0) return
    gl.useProgram(glyphProgram)
    gl.uniform2f(glyphLocs.uResolution, canvas.width, canvas.height)
    gl.uniform2f(glyphLocs.uAtlasResolution, ATLAS_SIZE, ATLAS_SIZE)
    gl.activeTexture(gl.TEXTURE0)
    gl.bindTexture(gl.TEXTURE_2D, atlasTexture)
    gl.uniform1i(glyphLocs.uAtlas, 0)
    gl.bindVertexArray(glyphVao)
    gl.bindBuffer(gl.ARRAY_BUFFER, glyphInstanceBuffer)
    gl.drawArraysInstanced(gl.TRIANGLES, 0, 6, total)
    gl.bindVertexArray(null)
  }

  function drawOverlayPass(): void {
    if (overlayInstanceData.length === 0) return
    gl.useProgram(solidProgram)
    gl.uniform2f(solidLocs.uResolution, canvas.width, canvas.height)
    gl.bindVertexArray(overlayVao)
    gl.bindBuffer(gl.ARRAY_BUFFER, overlayInstanceBuffer)
    gl.bufferData(gl.ARRAY_BUFFER, overlayInstanceData, gl.DYNAMIC_DRAW)
    gl.drawArraysInstanced(gl.TRIANGLES, 0, 6, overlayInstanceData.length / 8)
    gl.bindVertexArray(null)
  }

  function paintFrame(frame: TermyFrame): void {
    keyboardMode.applicationCursorKeys = frame.applicationCursorKeys

    // Overlay-only fast path: cursor blink / selection / link toggle without
    // any cell-state changes. We re-clear and replay the three layers, but
    // skip rewriting the bg + glyph instance buffers (they're already on the
    // GPU from the last paint).
    if (overlayDirty && !needsFullRepaint && lastSnapshot === frame) {
      buildOverlayData(frame)
      const [cr, cg, cb, ca] = bgClearColor()
      gl.viewport(0, 0, canvas.width, canvas.height)
      gl.clearColor(cr * ca, cg * ca, cb * ca, ca)
      gl.clear(gl.COLOR_BUFFER_BIT)
      drawBgPass(frame)
      drawGlyphPass(frame)
      drawOverlayPass()
      overlayDirty = false
      return
    }

    // Grid dimensions may have changed externally (resize). Re-allocate
    // persistent slot buffers if so.
    if (frame.cols * frame.rows !== slotCapacity) {
      ensureSlotCapacity()
    }

    let fullRepaint =
      needsFullRepaint ||
      !lastSnapshot ||
      lastSnapshot.cols !== frame.cols ||
      lastSnapshot.rows !== frame.rows ||
      lastSnapshot.displayOffset !== frame.displayOffset

    const total = frame.cols * frame.rows

    if (fullRepaint) {
      let allocFailed = false
      for (let i = 0; i < total; i++) {
        if (!writeSlot(frame, i)) {
          allocFailed = true
          break
        }
      }
      if (allocFailed) {
        // Atlas allocation failed mid-frame (extremely rare: huge glyph,
        // packer couldn't fit). Reset atlas and try once more.
        resetAtlas()
        for (let i = 0; i < total; i++) {
          writeSlot(frame, i)
        }
      }
      gl.bindBuffer(gl.ARRAY_BUFFER, bgInstanceBuffer)
      gl.bufferData(gl.ARRAY_BUFFER, bgSlotData, gl.DYNAMIC_DRAW)
      gl.bindBuffer(gl.ARRAY_BUFFER, glyphInstanceBuffer)
      gl.bufferData(gl.ARRAY_BUFFER, glyphSlotData, gl.DYNAMIC_DRAW)
    } else {
      // Diff against last snapshot and upload only dirty runs.
      const runs = computeDirtyRuns(frame)
      if (runs === null) {
        // computeDirtyRuns flagged a structural change — fall back to full.
        fullRepaint = true
      } else if (runs.length === 0) {
        // No cell changes; we still want to repaint the overlay below.
      } else if (runs.length === 1 && runs[0]![0] === 0 && runs[0]![1] === total - 1) {
        // Threshold-based full upload (computeDirtyRuns coalesced to one run).
        let allocFailed = false
        for (let i = 0; i < total; i++) {
          if (!writeSlot(frame, i)) {
            allocFailed = true
            break
          }
        }
        if (allocFailed) {
          resetAtlas()
          for (let i = 0; i < total; i++) writeSlot(frame, i)
        }
        gl.bindBuffer(gl.ARRAY_BUFFER, bgInstanceBuffer)
        gl.bufferData(gl.ARRAY_BUFFER, bgSlotData, gl.DYNAMIC_DRAW)
        gl.bindBuffer(gl.ARRAY_BUFFER, glyphInstanceBuffer)
        gl.bufferData(gl.ARRAY_BUFFER, glyphSlotData, gl.DYNAMIC_DRAW)
      } else {
        // Re-rasterize dirty cells, upload each run as a sub-buffer slice.
        let allocFailed = false
        for (const [start, end] of runs) {
          for (let i = start; i <= end; i++) {
            if (!writeSlot(frame, i)) {
              allocFailed = true
              break
            }
          }
          if (allocFailed) break
        }
        if (allocFailed) {
          // Fall back to a full repaint after an atlas reset.
          resetAtlas()
          for (let i = 0; i < total; i++) writeSlot(frame, i)
          gl.bindBuffer(gl.ARRAY_BUFFER, bgInstanceBuffer)
          gl.bufferData(gl.ARRAY_BUFFER, bgSlotData, gl.DYNAMIC_DRAW)
          gl.bindBuffer(gl.ARRAY_BUFFER, glyphInstanceBuffer)
          gl.bufferData(gl.ARRAY_BUFFER, glyphSlotData, gl.DYNAMIC_DRAW)
        } else {
          gl.bindBuffer(gl.ARRAY_BUFFER, bgInstanceBuffer)
          for (const [start, end] of runs) {
            const byteOff = start * SOLID_FLOATS_PER_INSTANCE * 4
            const view = bgSlotData.subarray(
              start * SOLID_FLOATS_PER_INSTANCE,
              (end + 1) * SOLID_FLOATS_PER_INSTANCE,
            )
            gl.bufferSubData(gl.ARRAY_BUFFER, byteOff, view)
          }
          gl.bindBuffer(gl.ARRAY_BUFFER, glyphInstanceBuffer)
          for (const [start, end] of runs) {
            const byteOff = start * GLYPH_FLOATS_PER_INSTANCE * 4
            const view = glyphSlotData.subarray(
              start * GLYPH_FLOATS_PER_INSTANCE,
              (end + 1) * GLYPH_FLOATS_PER_INSTANCE,
            )
            gl.bufferSubData(gl.ARRAY_BUFFER, byteOff, view)
          }
        }
      }
    }

    // Update detectedLinks BEFORE buildOverlayData so always-on OSC8
    // underlines reflect the current frame.
    detectedLinks = detectLinks(frame)
    buildOverlayData(frame)

    const [cr, cg, cb, ca] = bgClearColor()
    gl.viewport(0, 0, canvas.width, canvas.height)
    gl.clearColor(cr * ca, cg * ca, cb * ca, ca)
    gl.clear(gl.COLOR_BUFFER_BIT)

    drawBgPass(frame)
    drawGlyphPass(frame)
    drawOverlayPass()

    lastSnapshot = frame
    needsFullRepaint = false
    overlayDirty = false
  }

  function scheduleRender(): void {
    needsRender = true
    if (rafHandle !== null) return
    const fps = init.options.rendererFps ?? DEFAULT_FPS
    const frameInterval = 1000 / Math.max(1, fps)

    rafHandle = requestAnimationFrame((time) => {
      rafHandle = null
      if (disposed) return
      if (time - lastRenderTime < frameInterval) {
        scheduleRender()
        return
      }
      if (!needsRender) return
      needsRender = false
      lastRenderTime = time
      paintFrame(init.core.snapshot())
      // Keep advancing while the bell flash is still animating.
      if (bell.isFlashing(time)) {
        scheduleOverlayRender()
      }
    })
  }

  // Overlay-only repaint requested (no cell-state changes).
  function scheduleOverlayRender(): void {
    overlayDirty = true
    needsRender = true
    if (rafHandle !== null) return
    const fps = init.options.rendererFps ?? DEFAULT_FPS
    const frameInterval = 1000 / Math.max(1, fps)

    rafHandle = requestAnimationFrame((time) => {
      rafHandle = null
      if (disposed) return
      if (time - lastRenderTime < frameInterval) {
        scheduleOverlayRender()
        return
      }
      if (!needsRender) return
      needsRender = false
      lastRenderTime = time
      // Reuse the cached snapshot if we have one — the cell state hasn't
      // changed since the last full paint. If we don't have one yet (very
      // first frame), fall through to a real snapshot + full paint.
      const frame = lastSnapshot ?? init.core.snapshot()
      paintFrame(frame)
      if (bell.isFlashing(time)) {
        scheduleOverlayRender()
      }
    })
  }

  function fireInput(payload: Uint8Array): void {
    for (const listener of inputListeners) {
      listener(payload)
    }
  }

  function fireSelectionChange(): void {
    if (!lastSnapshot) return
    const range = selection.getRange(lastSnapshot)
    const text = selection.getText(lastSnapshot)
    const payload: SelectionPayload = range
      ? {
          active: true,
          startRow: range.startRow,
          startCol: range.startCol,
          endRow: range.endRow,
          endCol: range.endCol,
          text,
        }
      : { active: false, startRow: 0, startCol: 0, endRow: 0, endCol: 0, text: '' }
    for (const listener of selectionListeners) {
      listener(payload)
    }
  }

  function pointerToCell(event: PointerEvent): { row: number; col: number } | null {
    const rect = canvas.getBoundingClientRect()
    const x = event.clientX - rect.left - renderConfig.paddingX
    const y = event.clientY - rect.top - renderConfig.paddingY
    if (x < 0 || y < 0) return null
    const col = Math.min(cols - 1, Math.max(0, Math.floor(x / cellWidth)))
    const row = Math.min(rows - 1, Math.max(0, Math.floor(y / cellHeight)))
    return { row, col }
  }

  function modifiersFromEvent(event: PointerEvent | MouseEvent): LinkPayload['modifiers'] {
    return {
      control: event.ctrlKey,
      alt: event.altKey,
      shift: event.shiftKey,
      meta: event.metaKey,
    }
  }

  function handlePointerDown(event: PointerEvent): void {
    if (event.button !== 0) return
    const cell = pointerToCell(event)
    if (!cell) return

    const modifiers = modifiersFromEvent(event)
    if (hoveredLink && (modifiers.meta || modifiers.control)) {
      for (const listener of linkListeners) {
        listener({
          uri: hoveredLink.uri,
          row: hoveredLink.row,
          startCol: hoveredLink.startCol,
          endCol: hoveredLink.endCol,
          modifiers,
        })
      }
      return
    }

    const now = performance.now()
    if (now - lastClickTime < MULTI_CLICK_MS) {
      lastClickCount = Math.min(3, lastClickCount + 1)
    } else {
      lastClickCount = 1
    }
    lastClickTime = now

    const mode: SelectionMode =
      lastClickCount === 1 ? 'cell' : lastClickCount === 2 ? 'word' : 'line'
    selection.start(cell.row, cell.col, mode)
    dragging = mode === 'cell'
    canvas.setPointerCapture(event.pointerId)
    // Selection lives in the overlay layer; no need to rebuild cell buffers.
    scheduleOverlayRender()
    fireSelectionChange()
  }

  function handlePointerMove(event: PointerEvent): void {
    const cell = pointerToCell(event)
    if (!cell) return

    if (dragging && selection.isActive()) {
      selection.extend(cell.row, cell.col)
      scheduleOverlayRender()
      fireSelectionChange()
    }

    const modifiers = modifiersFromEvent(event)
    const link =
      modifiers.meta || modifiers.control ? findLinkAt(detectedLinks, cell.row, cell.col) : null
    if (link !== hoveredLink) {
      hoveredLink = link
      canvas.style.cursor = link ? 'pointer' : ''
      scheduleOverlayRender()
    }
  }

  function handlePointerUp(event: PointerEvent): void {
    if (dragging) {
      dragging = false
      try {
        canvas.releasePointerCapture(event.pointerId)
      } catch {}
      fireSelectionChange()
    }
  }

  function handleWheel(event: WheelEvent): void {
    if (event.deltaY === 0) return
    event.preventDefault()
    const lines =
      Math.sign(event.deltaY) * Math.max(1, Math.round(Math.abs(event.deltaY) / cellHeight))
    init.core.scrollLines(-lines)
    // Scrolling changes displayOffset; force full repaint.
    needsFullRepaint = true
    scheduleRender()
  }

  canvas.addEventListener('pointerdown', handlePointerDown)
  canvas.addEventListener('pointermove', handlePointerMove)
  canvas.addEventListener('pointerup', handlePointerUp)
  canvas.addEventListener('pointercancel', handlePointerUp)
  canvas.addEventListener('wheel', handleWheel, { passive: false })

  const cursorBlinkEnabled = init.options.cursorBlink ?? renderConfig.cursorBlink
  if (cursorBlinkEnabled) {
    cursorBlinkTimer = setInterval(() => {
      cursorVisible = !cursorVisible
      // Cursor blink only affects the overlay layer.
      scheduleOverlayRender()
    }, 500)
  }

  const domInput: DomInputController = attachDomInput({
    host: init.host,
    bindings: {
      onInput: fireInput,
      getKeyboardMode: () => keyboardMode,
      isMacOption: () => init.options.macOptionIsMeta ?? false,
      isBracketedPaste: () => init.core.bracketedPaste(),
    },
  })

  const resizeObserver =
    typeof ResizeObserver !== 'undefined'
      ? new ResizeObserver(() => {
          recomputeGrid()
          scheduleRender()
        })
      : null
  resizeObserver?.observe(init.host)

  const onWorkerSnapshot = (): void => {
    needsFullRepaint = true
    needsRender = true
    scheduleRender()
  }
  init.host.addEventListener('termy:snapshot', onWorkerSnapshot)

  recomputeGrid()
  scheduleRender()

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
      if (disposed) throw new Error('TermyRenderer has been disposed')
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
      scheduleRender()
      return result
    },

    resize(nextCols, nextRows) {
      if (nextCols !== undefined || nextRows !== undefined) {
        cols = nextCols ?? cols
        rows = nextRows ?? rows
        init.core.resize(cols, rows, cellWidth, cellHeight)
        for (const listener of resizeListeners) {
          listener({ cols, rows })
        }
        needsFullRepaint = true
        ensureSlotCapacity()
        scheduleRender()
      } else {
        recomputeGrid()
        scheduleRender()
      }
    },

    fit() {
      return { cols, rows }
    },

    focus() {
      domInput.focus()
    },

    blur() {
      domInput.blur()
    },

    scrollToBottom() {
      init.core.scrollToBottom()
      needsFullRepaint = true
      scheduleRender()
    },

    scrollLines(amount: number) {
      init.core.scrollLines(amount)
      needsFullRepaint = true
      scheduleRender()
    },

    getSelection() {
      if (!lastSnapshot) return ''
      return selection.getText(lastSnapshot)
    },

    clearSelection() {
      if (!selection.isActive()) return
      selection.clear()
      scheduleOverlayRender()
      fireSelectionChange()
    },

    async copy() {
      const text = lastSnapshot ? selection.getText(lastSnapshot) : ''
      if (typeof navigator !== 'undefined' && navigator.clipboard && text) {
        await navigator.clipboard.writeText(text)
      }
    },

    paste(text) {
      fireInput(TEXT_ENCODER.encode(text))
    },

    snapshot() {
      return init.core.snapshot()
    },

    search(query, _options?: SearchOptions) {
      return init.core.search(query)
    },

    searchAndHighlight(query: string, _options?: SearchOptions): TermySearchMatch[] {
      const matches = init.core.search(query)
      searchHighlight.setQuery(query, matches)
      // Search highlights live in the overlay layer.
      scheduleOverlayRender()
      return matches
    },

    findNextMatch(): TermySearchMatch | null {
      const match = searchHighlight.next()
      scheduleOverlayRender()
      return match
    },

    findPreviousMatch(): TermySearchMatch | null {
      const match = searchHighlight.previous()
      scheduleOverlayRender()
      return match
    },

    clearSearchHighlight(): void {
      if (!searchHighlight.isActive() && searchHighlight.query === null) return
      searchHighlight.clear()
      scheduleOverlayRender()
    },

    serialize() {
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
      if (rafHandle !== null) {
        cancelAnimationFrame(rafHandle)
        rafHandle = null
      }
      if (cursorBlinkTimer !== null) {
        clearInterval(cursorBlinkTimer)
        cursorBlinkTimer = null
      }
      canvas.removeEventListener('pointerdown', handlePointerDown)
      canvas.removeEventListener('pointermove', handlePointerMove)
      canvas.removeEventListener('pointerup', handlePointerUp)
      canvas.removeEventListener('pointercancel', handlePointerUp)
      canvas.removeEventListener('wheel', handleWheel)
      resizeObserver?.disconnect()
      init.host.removeEventListener('termy:snapshot', onWorkerSnapshot)
      domInput.dispose()
      inputListeners.clear()
      resizeListeners.clear()
      selectionListeners.clear()
      linkListeners.clear()
      if (atlasTexture) {
        gl.deleteTexture(atlasTexture)
        atlasTexture = null
      }
      gl.deleteBuffer(quadBuffer)
      gl.deleteBuffer(glyphInstanceBuffer)
      gl.deleteBuffer(bgInstanceBuffer)
      gl.deleteBuffer(overlayInstanceBuffer)
      gl.deleteVertexArray(glyphVao)
      gl.deleteVertexArray(bgVao)
      gl.deleteVertexArray(overlayVao)
      gl.deleteProgram(glyphProgram)
      gl.deleteProgram(solidProgram)
      atlasEntries.clear()
      lruHead = null
      lruTail = null
      bell.dispose()
      void detectedLinks
      canvas.remove()
    },
  }
}

function createProgram(
  gl: WebGL2RenderingContext,
  vertexSrc: string,
  fragmentSrc: string,
): WebGLProgram {
  const vertex = compileShader(gl, gl.VERTEX_SHADER, vertexSrc)
  const fragment = compileShader(gl, gl.FRAGMENT_SHADER, fragmentSrc)
  const program = gl.createProgram()
  if (!program) {
    throw new Error('webgl2 backend could not create program')
  }
  gl.attachShader(program, vertex)
  gl.attachShader(program, fragment)
  gl.linkProgram(program)
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    const log = gl.getProgramInfoLog(program) ?? 'unknown error'
    gl.deleteProgram(program)
    gl.deleteShader(vertex)
    gl.deleteShader(fragment)
    throw new Error(`webgl2 backend program link failed: ${log}`)
  }
  gl.deleteShader(vertex)
  gl.deleteShader(fragment)
  return program
}

function compileShader(gl: WebGL2RenderingContext, type: number, src: string): WebGLShader {
  const shader = gl.createShader(type)
  if (!shader) {
    throw new Error('webgl2 backend could not create shader')
  }
  gl.shaderSource(shader, src)
  gl.compileShader(shader)
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    const log = gl.getShaderInfoLog(shader) ?? 'unknown error'
    gl.deleteShader(shader)
    throw new Error(`webgl2 backend shader compile failed: ${log}`)
  }
  return shader
}

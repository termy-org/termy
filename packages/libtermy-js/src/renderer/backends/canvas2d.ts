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
import { attachDomInput, type DomInputController } from '../dom-input'
import {
  createLifecycleDispatchers,
  dispatchTermyEvents,
  type LifecycleEventDispatchers,
} from '../events'
import { DEFAULT_KEYBOARD_MODE, type TerminalKeyboardMode } from '../keyboard'
import { detectLinks, findLinkAt, type DetectedLink } from '../links'
import {
  buttonCode,
  isSelectionOverride,
  modifierBitmask,
  MOUSE_BUTTON_CODE,
  MOUSE_EVENT_KIND_CODE,
  pointerButtonToProtocol,
  shouldReportMouseEvent,
  type MouseButton,
  type MouseEncoding,
  type MouseEventKind,
  type MouseMode,
  type MouseModifiers,
} from '../mouse'
import { paintSearchHighlight, SearchHighlightTracker } from '../search-highlight'
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

interface Canvas2dRendererInit {
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

const TEXT_ENCODER = new TextEncoder()
const GLYPH_CACHE_MAX = 4096
const DEFAULT_FPS = 60
const MULTI_CLICK_MS = 350

export function createCanvas2dRenderer(init: Canvas2dRendererInit): TermyRenderer {
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

  if (init.options.scrollback !== undefined) {
    init.core.setScrollback(init.options.scrollback)
  }

  const canvas = document.createElement('canvas')
  canvas.style.display = 'block'
  canvas.style.width = '100%'
  canvas.style.height = '100%'
  init.host.appendChild(canvas)

  const ctx = canvas.getContext('2d', { alpha: init.options.allowTransparency ?? true })
  if (!ctx) {
    throw new Error('canvas2d backend requires a 2D context')
  }

  const glyphCache = new Map<string, HTMLCanvasElement>()
  const glyphLru: string[] = []
  const selection = new SelectionTracker(init.options.wordSeparator)
  const searchHighlight = new SearchHighlightTracker()
  const bell = new BellHandler({ mode: init.options.bellSound ?? 'none' })
  const lifecycleDispatchers: LifecycleEventDispatchers = createLifecycleDispatchers()
  // Local bell listener that triggers the visual/audio bell. We register it
  // through the lifecycle dispatcher so external consumers can still subscribe
  // via `onBell` and observe the same event.
  lifecycleDispatchers.bell.add(() => bell.trigger())
  const keyboardMode: TerminalKeyboardMode = { ...DEFAULT_KEYBOARD_MODE }
  let detectedLinks: DetectedLink[] = []
  let hoveredLink: DetectedLink | null = null
  let lastClickTime = 0
  let lastClickCount = 0
  let dragging = false
  // Cached mouse reporting state. Updated on every paint from the snapshot
  // frame so the most recent `CSI ? … h/l` mode change is always in effect
  // by the time pointer events fire.
  let mouseMode: MouseMode = 'none'
  let mouseEncoding: MouseEncoding = 'legacy'
  // Tracks the protocol button currently held so we can emit drag events
  // with the correct button code. `none` means nothing is held.
  let activeMouseButton: MouseButton = 'none'
  // Whether the last pointer-down was consumed by mouse reporting (so the
  // matching up/move events should also be reported instead of starting a
  // selection drag).
  let reportingPointer = false

  function readDpr(): number {
    return typeof window !== 'undefined' ? window.devicePixelRatio || 1 : 1
  }

  function fontString(): string {
    const family = init.options.fontFamily ?? renderConfig.fontFamily
    const size = init.options.fontSize ?? renderConfig.fontSize
    return `${size}px ${family}`
  }

  // Resolve the CSS `font` shorthand for the requested style combo. CSS
  // accepts `italic` and `bold` in either order, but we standardize on
  // `<style> <weight> <size> <family>` to keep the cache key stable.
  function fontStringFor(bold: boolean, italic: boolean): string {
    const family = init.options.fontFamily ?? renderConfig.fontFamily
    const size = init.options.fontSize ?? renderConfig.fontSize
    const style = italic ? 'italic ' : ''
    const weight = bold ? 'bold ' : ''
    return `${style}${weight}${size}px ${family}`
  }

  function measureCell(): { width: number; height: number } {
    if (!ctx) return { width: cellWidth, height: cellHeight }
    ctx.font = fontString()
    const metrics = ctx.measureText('M')
    const width = metrics.width || cellWidth
    const lineHeight = init.options.lineHeight ?? renderConfig.lineHeight
    const baseSize = init.options.fontSize ?? renderConfig.fontSize
    const height = Math.max(1, Math.round(baseSize * lineHeight))
    return { width, height }
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

    canvas.width = Math.round(hostRect.width * devicePixelRatio)
    canvas.height = Math.round(hostRect.height * devicePixelRatio)
    canvas.style.width = `${hostRect.width}px`
    canvas.style.height = `${hostRect.height}px`
    if (ctx) {
      ctx.setTransform(devicePixelRatio, 0, 0, devicePixelRatio, 0, 0)
    }
    glyphCache.clear()
    glyphLru.length = 0
    lastSnapshot = null
    needsRender = true
  }

  function colorString(color: TermyColor): string {
    return `rgba(${color.r}, ${color.g}, ${color.b}, ${color.a / 255})`
  }

  // Same as colorString but with a per-call alpha multiplier. Used by `dim`
  // (renders fg at 60% alpha).
  function colorStringWithAlpha(color: TermyColor, alphaMul: number): string {
    return `rgba(${color.r}, ${color.g}, ${color.b}, ${(color.a / 255) * alphaMul})`
  }

  // Resolve the effective fg/bg for a cell after applying SGR `reverse`. We
  // don't touch dim here — dim is applied to alpha at paint time, not to the
  // logical color.
  function effectiveColors(cell: TermyCell): { fg: TermyColor; bg: TermyColor; bgIsDefault: boolean } {
    if (!cell.reverse) {
      return { fg: cell.fg, bg: cell.bg, bgIsDefault: cell.usesTerminalDefaultBg }
    }
    // Swap. If the original bg was the terminal default we materialize the
    // current bg color into the swapped fg so the glyph is visible. The
    // resulting bg is the cell's original fg (always concrete).
    const effFg = cell.usesTerminalDefaultBg ? renderConfig.background : cell.bg
    return { fg: effFg, bg: cell.fg, bgIsDefault: false }
  }

  function backgroundColorString(): string {
    const opacity = renderConfig.backgroundOpacity
    const bg = renderConfig.background
    return `rgba(${bg.r}, ${bg.g}, ${bg.b}, ${(bg.a / 255) * opacity})`
  }

  function cellKey(cell: TermyCell): string {
    // `width` is part of the key because wide glyphs (width=2) are rasterized
    // onto a 2*cellWidth canvas; a narrow cell must not reuse that bitmap.
    // Attributes are appended after the colors. Reverse video changes which
    // color lands in fg/bg, but we apply that swap before computing the key so
    // the cache reflects the actually-painted bitmap.
    const eff = effectiveColors(cell)
    const attrs = `${cell.bold ? 1 : 0}|${cell.italic ? 1 : 0}|${cell.underline ? 1 : 0}|${cell.strikethrough ? 1 : 0}|${cell.dim ? 1 : 0}|${cell.blink ? 1 : 0}|${cell.reverse ? 1 : 0}|${cell.invisible ? 1 : 0}`
    return `${cell.char}|${eff.fg.r},${eff.fg.g},${eff.fg.b}|${eff.bg.r},${eff.bg.g},${eff.bg.b},${eff.bgIsDefault ? 1 : 0}|${attrs}|${cell.renderText ? 1 : 0}|${cell.width}`
  }

  function renderGlyph(cell: TermyCell): HTMLCanvasElement {
    const key = cellKey(cell)
    const cached = glyphCache.get(key)
    if (cached) {
      touchLru(key)
      return cached
    }

    const colSpan = cell.width === 2 ? 2 : 1
    const glyph = document.createElement('canvas')
    glyph.width = Math.ceil(cellWidth * colSpan * devicePixelRatio)
    glyph.height = Math.ceil(cellHeight * devicePixelRatio)
    const gctx = glyph.getContext('2d')
    if (!gctx) return glyph
    gctx.setTransform(devicePixelRatio, 0, 0, devicePixelRatio, 0, 0)

    const eff = effectiveColors(cell)

    if (!eff.bgIsDefault) {
      gctx.fillStyle = colorString(eff.bg)
      gctx.fillRect(0, 0, cellWidth * colSpan, cellHeight)
    }

    // `invisible` keeps the bg fill but skips the glyph + decorations. The
    // cursor still lands in the right place, so the cell is "there" but
    // empty-looking.
    if (cell.invisible) {
      glyphCache.set(key, glyph)
      glyphLru.push(key)
      if (glyphLru.length > GLYPH_CACHE_MAX) {
        const evicted = glyphLru.shift()
        if (evicted) glyphCache.delete(evicted)
      }
      return glyph
    }

    // Dim renders the fg at 60% alpha. Applies to the glyph itself and any
    // underline / strikethrough decorations so they don't visually outshout
    // the text they belong to.
    const fgAlpha = cell.dim ? 0.6 : 1
    const fgStyle = fgAlpha < 1 ? colorStringWithAlpha(eff.fg, fgAlpha) : colorString(eff.fg)

    if (cell.renderText && cell.char.trim().length > 0) {
      gctx.font = fontStringFor(cell.bold, cell.italic)
      gctx.fillStyle = fgStyle
      gctx.textBaseline = 'top'
      const baseSize = init.options.fontSize ?? renderConfig.fontSize
      const yOffset = (cellHeight - baseSize) / 2
      gctx.fillText(cell.char, 0, yOffset)
    }

    // Underline: 1px line near the cell's baseline. Painted across the full
    // cell width so consecutive underlined cells form one continuous run.
    if (cell.underline) {
      gctx.fillStyle = fgStyle
      gctx.fillRect(0, cellHeight - 1, cellWidth * colSpan, 1)
    }

    // Strikethrough: 1px line through the vertical middle of the cell.
    if (cell.strikethrough) {
      gctx.fillStyle = fgStyle
      gctx.fillRect(0, Math.floor(cellHeight / 2), cellWidth * colSpan, 1)
    }

    // Blink: TODO — wire a blink timer that swaps `cell.blink` cells between
    // visible and invisible at ~500ms intervals. Most modern terminals (iTerm
    // default, xterm, kitty) render blink as a no-op, so we follow suit for v1.

    glyphCache.set(key, glyph)
    glyphLru.push(key)
    if (glyphLru.length > GLYPH_CACHE_MAX) {
      const evicted = glyphLru.shift()
      if (evicted) glyphCache.delete(evicted)
    }
    return glyph
  }

  function touchLru(key: string): void {
    const idx = glyphLru.indexOf(key)
    if (idx >= 0) {
      glyphLru.splice(idx, 1)
      glyphLru.push(key)
    }
  }

  function paintFrame(frame: TermyFrame): void {
    if (!ctx) return

    keyboardMode.applicationCursorKeys = frame.applicationCursorKeys
    mouseMode = frame.mouseMode
    mouseEncoding = frame.mouseEncoding
    void mouseEncoding

    const paddingX = renderConfig.paddingX
    const paddingY = renderConfig.paddingY
    const fullRepaint =
      !lastSnapshot ||
      lastSnapshot.cols !== frame.cols ||
      lastSnapshot.rows !== frame.rows ||
      lastSnapshot.displayOffset !== frame.displayOffset

    if (fullRepaint) {
      ctx.fillStyle = backgroundColorString()
      ctx.fillRect(0, 0, canvas.width / devicePixelRatio, canvas.height / devicePixelRatio)
    }

    for (let row = 0; row < frame.rows; row++) {
      for (let col = 0; col < frame.cols; col++) {
        const idx = row * frame.cols + col
        const cell = frame.cells[idx]
        if (!cell) continue
        // The right half of a wide glyph is rendered by its wide partner one
        // column to the left. Skip emitting anything for it.
        if (cell.width === 0) continue
        if (!fullRepaint && lastSnapshot) {
          const prev = lastSnapshot.cells[idx]
          if (prev && cellsEqual(prev, cell)) continue
        }
        const colSpan = cell.width === 2 ? 2 : 1
        const x = paddingX + col * cellWidth
        const y = paddingY + row * cellHeight
        const drawWidth = cellWidth * colSpan
        const eff = effectiveColors(cell)
        ctx.fillStyle = eff.bgIsDefault ? backgroundColorString() : colorString(eff.bg)
        ctx.fillRect(x, y, drawWidth, cellHeight)
        const glyph = renderGlyph(cell)
        ctx.drawImage(glyph, 0, 0, glyph.width, glyph.height, x, y, drawWidth, cellHeight)
      }
    }

    // Update detected links before painting OSC8 underlines so the always-on
    // overlay reflects the current frame.
    detectedLinks = detectLinks(frame)
    selection.bind(frame)

    paintSelection(frame, paddingX, paddingY)
    paintSearchHighlight(
      ctx,
      paddingX,
      paddingY,
      cellWidth,
      cellHeight,
      searchHighlight,
      frame,
      renderConfig,
    )
    paintOsc8Underlines(paddingX, paddingY)
    paintLinkUnderline(paddingX, paddingY)
    paintCursor(frame, paddingX, paddingY)
    paintBellFlash()
    lastSnapshot = frame
  }

  function paintBellFlash(): void {
    if (!ctx) return
    const now = typeof performance !== 'undefined' ? performance.now() : Date.now()
    const intensity = bell.visualIntensity(now)
    if (intensity <= 0) return
    const fg = renderConfig.foreground
    ctx.fillStyle = `rgba(${fg.r}, ${fg.g}, ${fg.b}, ${intensity})`
    const widthCss = canvas.width / devicePixelRatio
    const heightCss = canvas.height / devicePixelRatio
    ctx.fillRect(0, 0, widthCss, heightCss)
    // Keep the cell diff cache from short-circuiting future frames while the
    // flash is still decaying: clearing `lastSnapshot` forces a full repaint
    // next tick so the overlay shrinks correctly.
    lastSnapshot = null
  }

  function paintSelection(frame: TermyFrame, paddingX: number, paddingY: number): void {
    if (!ctx || !selection.isActive()) return
    const range = selection.getRange(frame)
    if (!range) return
    ctx.fillStyle = 'rgba(120, 150, 255, 0.35)'
    if (selection.isRectMode()) {
      const minCol = Math.min(range.startCol, range.endCol)
      const maxCol = Math.max(range.startCol, range.endCol)
      for (let row = range.startRow; row <= range.endRow; row++) {
        const x = paddingX + minCol * cellWidth
        const y = paddingY + row * cellHeight
        const w = (maxCol - minCol + 1) * cellWidth
        ctx.fillRect(x, y, w, cellHeight)
      }
      return
    }
    for (let row = range.startRow; row <= range.endRow; row++) {
      const fromCol = row === range.startRow ? range.startCol : 0
      const toCol = row === range.endRow ? range.endCol : frame.cols - 1
      const x = paddingX + fromCol * cellWidth
      const y = paddingY + row * cellHeight
      const w = (toCol - fromCol + 1) * cellWidth
      ctx.fillRect(x, y, w, cellHeight)
    }
  }

  function paintOsc8Underlines(paddingX: number, paddingY: number): void {
    if (!ctx) return
    ctx.strokeStyle = colorString(renderConfig.foreground)
    ctx.lineWidth = 1
    for (const link of detectedLinks) {
      if (link.source !== 'osc8') continue
      const x = paddingX + link.startCol * cellWidth
      const y = paddingY + (link.row + 1) * cellHeight - 1
      const w = (link.endCol - link.startCol + 1) * cellWidth
      ctx.beginPath()
      ctx.moveTo(x, y)
      ctx.lineTo(x + w, y)
      ctx.stroke()
    }
  }

  function paintLinkUnderline(paddingX: number, paddingY: number): void {
    if (!ctx || !hoveredLink) return
    // OSC8 hyperlinks are always underlined by `paintOsc8Underlines`; treat a
    // legacy `undefined` source as a regex link.
    const source = hoveredLink.source ?? 'regex'
    if (source !== 'regex') return
    ctx.strokeStyle = colorString(renderConfig.foreground)
    ctx.lineWidth = 1
    const x = paddingX + hoveredLink.startCol * cellWidth
    const y = paddingY + (hoveredLink.row + 1) * cellHeight - 1
    const w = (hoveredLink.endCol - hoveredLink.startCol + 1) * cellWidth
    ctx.beginPath()
    ctx.moveTo(x, y)
    ctx.lineTo(x + w, y)
    ctx.stroke()
  }

  function paintCursor(frame: TermyFrame, paddingX: number, paddingY: number): void {
    if (!ctx || !frame.cursor || !cursorVisible) return
    const x = paddingX + frame.cursor.col * cellWidth
    const y = paddingY + frame.cursor.row * cellHeight
    ctx.fillStyle = colorString(renderConfig.cursor)
    if (frame.cursor.style === 'line') {
      ctx.fillRect(x, y, Math.max(1, cellWidth / 8), cellHeight)
    } else {
      ctx.fillRect(x, y, cellWidth, cellHeight)
    }
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
      a.bg.r === b.bg.r &&
      a.bg.g === b.bg.g &&
      a.bg.b === b.bg.b
    )
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
      // A bell flash animates over ~150ms, so we keep requesting frames
      // until the intensity decays to zero.
      if (bell.isFlashing(time)) {
        scheduleRender()
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

  function mouseModifiersFromEvent(event: PointerEvent | MouseEvent | WheelEvent): MouseModifiers {
    return {
      shift: event.shiftKey,
      alt: event.altKey,
      control: event.ctrlKey,
    }
  }

  function emitMouseReport(input: {
    button: MouseButton
    kind: MouseEventKind
    col: number
    row: number
    modifiers: MouseModifiers
  }): boolean {
    if (!shouldReportMouseEvent(mouseMode, input.kind)) return false
    const button = buttonCode(input.button)
    const modifiers = modifierBitmask(input.modifiers)
    const kindCode = MOUSE_EVENT_KIND_CODE[input.kind]
    const bytes = init.core.encodeMouseReport(
      button,
      modifiers,
      input.col,
      input.row,
      kindCode,
    )
    if (!bytes || bytes.length === 0) return false
    fireInput(bytes)
    return true
  }

  function handlePointerDown(event: PointerEvent): void {
    const cell = pointerToCell(event)
    if (!cell) return

    const modifiers = modifiersFromEvent(event)

    // Cmd/Ctrl-click on a detected URL: fire link callback regardless of
    // whether mouse reporting is active. (Selection wouldn't make sense here.)
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

    // xterm convention: shift always disables mouse reporting so users can
    // still text-select inside a TUI.
    const reportingActive =
      mouseMode !== 'none' && !isSelectionOverride({ shift: modifiers.shift })

    if (reportingActive) {
      const button = pointerButtonToProtocol(event.button)
      if (button === 'none') return
      const reported = emitMouseReport({
        button,
        kind: 'down',
        col: cell.col,
        row: cell.row,
        modifiers: mouseModifiersFromEvent(event),
      })
      if (reported) {
        activeMouseButton = button
        reportingPointer = true
        canvas.setPointerCapture(event.pointerId)
        return
      }
    }

    // Fall through to selection behaviour (only primary button starts a drag).
    if (event.button !== 0) return

    const now = performance.now()
    if (now - lastClickTime < MULTI_CLICK_MS) {
      lastClickCount = Math.min(3, lastClickCount + 1)
    } else {
      lastClickCount = 1
    }
    lastClickTime = now

    const mode: SelectionMode = event.altKey
      ? 'rect'
      : lastClickCount === 1
        ? 'cell'
        : lastClickCount === 2
          ? 'word'
          : 'line'
    const currentFrame = lastSnapshot ?? init.core.snapshot()
    selection.start(cell.row, cell.col, mode, currentFrame)
    dragging = mode === 'cell' || mode === 'rect'
    canvas.setPointerCapture(event.pointerId)
    needsRender = true
    scheduleRender()
    fireSelectionChange()
  }

  function handlePointerMove(event: PointerEvent): void {
    const cell = pointerToCell(event)
    if (!cell) return

    // If a mouse-reported press is in flight, route follow-up motion through
    // the protocol rather than extending a selection.
    if (reportingPointer && activeMouseButton !== 'none') {
      emitMouseReport({
        button: activeMouseButton,
        kind: 'drag',
        col: cell.col,
        row: cell.row,
        modifiers: mouseModifiersFromEvent(event),
      })
      return
    }

    // Bare motion (no button held) when in any-event mode.
    if (mouseMode === 'any-event' && !reportingPointer && !event.shiftKey) {
      emitMouseReport({
        button: 'none',
        kind: 'move',
        col: cell.col,
        row: cell.row,
        modifiers: mouseModifiersFromEvent(event),
      })
      // Bare motion does not suppress hover/selection logic below.
    }

    if (dragging && selection.isActive()) {
      selection.extend(cell.row, cell.col, lastSnapshot ?? undefined)
      needsRender = true
      scheduleRender()
      fireSelectionChange()
    }

    const modifiers = modifiersFromEvent(event)
    const link = modifiers.meta || modifiers.control ? findLinkAt(detectedLinks, cell.row, cell.col) : null
    if (link !== hoveredLink) {
      hoveredLink = link
      canvas.style.cursor = link ? 'pointer' : ''
      needsRender = true
      scheduleRender()
    }
  }

  function handlePointerUp(event: PointerEvent): void {
    if (reportingPointer) {
      const cell = pointerToCell(event) ?? { row: 0, col: 0 }
      emitMouseReport({
        button: activeMouseButton === 'none' ? 'left' : activeMouseButton,
        kind: 'up',
        col: cell.col,
        row: cell.row,
        modifiers: mouseModifiersFromEvent(event),
      })
      reportingPointer = false
      activeMouseButton = 'none'
      try {
        canvas.releasePointerCapture(event.pointerId)
      } catch {}
      return
    }

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

    // When mouse reporting is active, encode the wheel as buttons 64/65 so
    // TUIs (vim, htop, less, etc.) receive scroll events.
    if (mouseMode !== 'none' && !event.shiftKey) {
      event.preventDefault()
      const cell = pointerToCellFromMouseEvent(event)
      if (!cell) return
      const button: MouseButton = event.deltaY < 0 ? 'wheel-up' : 'wheel-down'
      emitMouseReport({
        button,
        kind: 'down',
        col: cell.col,
        row: cell.row,
        modifiers: mouseModifiersFromEvent(event),
      })
      return
    }

    event.preventDefault()
    const lines = Math.sign(event.deltaY) * Math.max(1, Math.round(Math.abs(event.deltaY) / cellHeight))
    init.core.scrollLines(-lines)
    needsRender = true
    scheduleRender()
  }

  function pointerToCellFromMouseEvent(event: MouseEvent): { row: number; col: number } | null {
    const rect = canvas.getBoundingClientRect()
    const x = event.clientX - rect.left - renderConfig.paddingX
    const y = event.clientY - rect.top - renderConfig.paddingY
    if (x < 0 || y < 0) return null
    const col = Math.min(cols - 1, Math.max(0, Math.floor(x / cellWidth)))
    const row = Math.min(rows - 1, Math.max(0, Math.floor(y / cellHeight)))
    return { row, col }
  }

  // Reference unused constant so the bundler keeps it discoverable from the
  // module's named exports while still allowing tree-shaking elsewhere.
  void MOUSE_BUTTON_CODE

  function handleHostClickOutside(): void {
    if (selection.isActive()) {
      selection.clear()
      needsRender = true
      scheduleRender()
      fireSelectionChange()
    }
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
      needsRender = true
      scheduleRender()
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
      scheduleRender()
    },

    scrollLines(amount: number) {
      init.core.scrollLines(amount)
      scheduleRender()
    },

    getSelection() {
      if (!lastSnapshot) return ''
      return selection.getText(lastSnapshot)
    },

    clearSelection() {
      if (!selection.isActive()) return
      selection.clear()
      needsRender = true
      scheduleRender()
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
      // Force a full repaint so the overlay lands cleanly over every cell.
      lastSnapshot = null
      needsRender = true
      scheduleRender()
      return matches
    },

    findNextMatch(): TermySearchMatch | null {
      const match = searchHighlight.next()
      lastSnapshot = null
      needsRender = true
      scheduleRender()
      return match
    },

    findPreviousMatch(): TermySearchMatch | null {
      const match = searchHighlight.previous()
      lastSnapshot = null
      needsRender = true
      scheduleRender()
      return match
    },

    clearSearchHighlight(): void {
      if (!searchHighlight.isActive() && searchHighlight.query === null) return
      searchHighlight.clear()
      // Force a full repaint so the old overlay rects are erased.
      lastSnapshot = null
      needsRender = true
      scheduleRender()
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
      glyphCache.clear()
      glyphLru.length = 0
      bell.dispose()
      canvas.remove()
      void handleHostClickOutside
    },
  }
}

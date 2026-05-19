import type { TermyFrame, TermyRenderConfig, TermySearchMatch } from '../index'

/**
 * Tracks the most recent search query, its match list, and which match is
 * currently "active" (the one navigated to by next/previous). The tracker is
 * deliberately ignorant of the canvas/painting layer — backends only need to
 * read state from it.
 */
export class SearchHighlightTracker {
  query: string | null = null
  matches: TermySearchMatch[] = []
  activeIndex: number = 0

  /**
   * Replace the current query and match set. `activeIndex` resets to 0 so
   * the first hit is highlighted by default. Passing `null`/empty matches
   * clears the highlight state.
   */
  setQuery(query: string | null, matches: TermySearchMatch[]): void {
    if (!query || matches.length === 0) {
      this.query = query ?? null
      this.matches = matches.slice()
      this.activeIndex = 0
      if (!query) {
        // Fully cleared.
        this.matches = []
      }
      return
    }
    this.query = query
    this.matches = matches.slice()
    this.activeIndex = 0
  }

  /** Advance to the next match, wrapping at the end. Returns the new active match, or null when empty. */
  next(): TermySearchMatch | null {
    if (this.matches.length === 0) return null
    this.activeIndex = (this.activeIndex + 1) % this.matches.length
    return this.matches[this.activeIndex] ?? null
  }

  /** Step to the previous match, wrapping at the start. Returns the new active match, or null when empty. */
  previous(): TermySearchMatch | null {
    if (this.matches.length === 0) return null
    this.activeIndex =
      (this.activeIndex - 1 + this.matches.length) % this.matches.length
    return this.matches[this.activeIndex] ?? null
  }

  /** Drop all search state. */
  clear(): void {
    this.query = null
    this.matches = []
    this.activeIndex = 0
  }

  getMatches(): TermySearchMatch[] {
    return this.matches
  }

  getActiveIndex(): number {
    return this.activeIndex
  }

  /** True when a query is set AND we have at least one match to paint. */
  isActive(): boolean {
    return this.query !== null && this.matches.length > 0
  }

  /** Returns the currently focused match, or null if no matches. */
  getActiveMatch(): TermySearchMatch | null {
    if (this.matches.length === 0) return null
    return this.matches[this.activeIndex] ?? null
  }
}

/**
 * Paint all matches as a low-alpha rounded fill, and stroke a 1px outline
 * around the active match so users can spot it during next/previous navigation.
 *
 * Matches referencing rows outside the visible frame are skipped silently —
 * scrollback or new output can shift rows out from under us between
 * `searchAndHighlight` and the next paint. We accept that desync rather than
 * trying to rerun the search on every write.
 */
export function paintSearchHighlight(
  ctx: CanvasRenderingContext2D,
  paddingX: number,
  paddingY: number,
  cellWidth: number,
  cellHeight: number,
  tracker: SearchHighlightTracker,
  frame: TermyFrame,
  renderConfig?: TermyRenderConfig,
): void {
  if (!tracker.isActive()) return
  const matches = tracker.getMatches()
  const activeIndex = tracker.getActiveIndex()

  const fillColor = 'rgba(255, 214, 102, 0.35)'
  // Default stroke is the configured foreground so the active outline sits
  // on top of any theme; fall back to a warm white if no config was provided.
  let strokeColor = 'rgba(255, 235, 180, 0.95)'
  if (renderConfig?.foreground) {
    const fg = renderConfig.foreground
    strokeColor = `rgba(${fg.r}, ${fg.g}, ${fg.b}, ${(fg.a / 255).toFixed(3)})`
  }

  ctx.save()
  for (let i = 0; i < matches.length; i++) {
    const match = matches[i]
    if (!match) continue
    if (match.row < 0 || match.row >= frame.rows) continue

    const startCol = Math.max(0, match.startCol)
    const endCol = Math.min(frame.cols - 1, match.endCol)
    if (endCol < startCol) continue

    const x = paddingX + startCol * cellWidth
    const y = paddingY + match.row * cellHeight
    const w = (endCol - startCol + 1) * cellWidth
    const h = cellHeight

    ctx.fillStyle = fillColor
    ctx.fillRect(x, y, w, h)

    if (i === activeIndex) {
      ctx.strokeStyle = strokeColor
      ctx.lineWidth = 1
      // Inset by 0.5px so the 1px stroke lands on integer pixels.
      ctx.strokeRect(x + 0.5, y + 0.5, Math.max(0, w - 1), Math.max(0, h - 1))
    }
  }
  ctx.restore()
}

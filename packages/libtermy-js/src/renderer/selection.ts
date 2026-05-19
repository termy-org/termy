import type { TermyFrame } from '../index'

export type SelectionMode = 'cell' | 'word' | 'line' | 'rect'

export interface SelectionRange {
  startRow: number
  startCol: number
  endRow: number
  endCol: number
}

export interface SelectionState {
  active: boolean
  mode: SelectionMode
  // Anchor/head are stored as ABSOLUTE buffer rows when `absolute` is true,
  // and as viewport-local rows when `absolute` is false (legacy callers that
  // do not pass a `frame` to start/extend).
  absolute: boolean
  anchorRow: number
  anchorCol: number
  headRow: number
  headCol: number
}

const DEFAULT_WORD_SEPARATORS = ' \t\n\r"\'`(){}[]<>;,'

/**
 * Convert a viewport-local row to an absolute buffer row.
 *
 *   absoluteRow = historySize - displayOffset + viewportRow
 *
 * `historySize` is the total scrollback row count, `displayOffset` is how far
 * we've scrolled back into history (0 means viewing the live tail).
 */
function viewportToAbsolute(frame: TermyFrame, viewportRow: number): number {
  return frame.historySize - frame.displayOffset + viewportRow
}

/**
 * Convert an absolute buffer row to a viewport-local row. May return a value
 * outside `[0, frame.rows)` if the row is off-screen.
 */
function absoluteToViewport(frame: TermyFrame, absoluteRow: number): number {
  return absoluteRow - (frame.historySize - frame.displayOffset)
}

export class SelectionTracker {
  private state: SelectionState | null = null
  private separators: Set<string>
  // Last `historySize` we observed in `bind()`. Used to detect scrollback
  // eviction: if historySize drops below this snapshot the buffer must have
  // shed older rows, which may include the anchor.
  private lastHistorySize: number | null = null

  constructor(wordSeparator: string = DEFAULT_WORD_SEPARATORS) {
    this.separators = new Set(wordSeparator.split(''))
  }

  /**
   * Begin a selection. If `frame` is provided, the (row, col) is interpreted
   * as viewport-local and translated to absolute buffer coordinates so the
   * selection survives scrolling. Otherwise (legacy) the coords are stored
   * as-is and treated as viewport-local.
   *
   * If a selection is already active and the new (row, col) matches the
   * current anchor exactly (in the same coordinate space), this is a no-op.
   * That preserves the existing mode (e.g. a double-click word selection)
   * when the user clicks directly on the original anchor cell instead of
   * resetting to a fresh cell selection.
   */
  start(row: number, col: number, mode: SelectionMode, frame?: TermyFrame): void {
    const absolute = frame !== undefined
    const storedRow = absolute ? viewportToAbsolute(frame, row) : row
    // Anchor-click guard: clicking the same anchor in the same coord space
    // shouldn't tear down an existing selection. Coord-space mismatches
    // (legacy callers mixing with frame-aware ones) fall through and reset
    // as before.
    if (
      this.state &&
      this.state.absolute === absolute &&
      this.state.anchorRow === storedRow &&
      this.state.anchorCol === col
    ) {
      return
    }
    this.state = {
      active: true,
      mode,
      absolute,
      anchorRow: storedRow,
      anchorCol: col,
      headRow: storedRow,
      headCol: col,
    }
  }

  /**
   * Update the head of an active selection. Mirrors `start`: when a `frame`
   * is provided we convert viewport→absolute, otherwise we store viewport
   * coords directly. The mode (rect/cell/word/line) is preserved.
   *
   * This is also the right entry point for shift-click flows: the anchor is
   * left untouched and only the head moves, which is exactly what xterm.js
   * and Terminal.app do when a user shift-clicks to extend a selection.
   * `extendFromAnchor` is a clearer-named alias for that use case.
   */
  extend(row: number, col: number, frame?: TermyFrame): void {
    if (!this.state) return
    const storedRow = frame !== undefined && this.state.absolute ? viewportToAbsolute(frame, row) : row
    this.state.headRow = storedRow
    this.state.headCol = col
  }

  /**
   * Shift-click extension. Behaviourally identical to `extend`: anchor is
   * preserved, only the head moves, and the current selection mode is kept.
   * Provided as a separate method so call sites that wire up shift-click
   * read clearly at the point of use.
   *
   * No-op when no selection is active — shift-click should only extend an
   * existing selection, never spawn a new one. Callers that want
   * shift-click to start a fresh selection from the cursor should do that
   * explicitly via `start`.
   */
  extendFromAnchor(row: number, col: number, frame?: TermyFrame): void {
    if (!this.state) return
    this.extend(row, col, frame)
  }

  /**
   * Refresh internal state against the latest frame. If the anchor row was
   * stored as an absolute buffer row and has since been evicted from
   * scrollback (i.e. it now points to a row that no longer exists because
   * `historySize` has shrunk), drop the selection. Safe to call every frame.
   */
  bind(frame: TermyFrame): void {
    // We always update the history-size watermark, even when there's no
    // active selection, so the first selection after bind() has a baseline.
    const prevHistorySize = this.lastHistorySize
    this.lastHistorySize = frame.historySize
    if (!this.state) return
    if (!this.state.absolute) return
    // Scrollback eviction: if historySize shrank past the row we anchored
    // on, the row is gone. The anchor was captured at absolute index
    //   anchorRow = historyAtCapture - displayOffsetAtCapture + viewportRow
    // so any drop in historySize that puts an endpoint below 0 is an
    // eviction. Equivalent check: endpoint < 0 (in absolute coords any
    // negative row was evicted).
    const { anchorRow, headRow } = this.state
    const maxAbsolute = frame.historySize + frame.rows - 1
    if (
      anchorRow < 0 ||
      headRow < 0 ||
      anchorRow > maxAbsolute ||
      headRow > maxAbsolute ||
      // historySize decreased AND that decrease crossed our anchor: drop.
      (prevHistorySize !== null &&
        frame.historySize < prevHistorySize &&
        anchorRow > frame.historySize + frame.rows - 1)
    ) {
      this.state = null
    }
  }

  clear(): void {
    this.state = null
  }

  isActive(): boolean {
    return this.state !== null
  }

  getMode(): SelectionMode | null {
    return this.state?.mode ?? null
  }

  isRectMode(): boolean {
    return this.state?.mode === 'rect'
  }

  /**
   * Compute the normalized selection range expressed in viewport-local
   * coordinates relative to `frame`. For modes 'cell'/'word'/'line' the
   * returned range follows the usual line-wrap convention (startRow uses
   * startCol, endRow uses endCol, intermediate rows span the full width).
   * For 'rect' the range is the bounding box and every row in
   * [startRow, endRow] is selected from startCol..endCol.
   *
   * If the selection is stored in absolute coords and an endpoint is
   * currently off-screen, the row is clamped to [0, frame.rows - 1]. Cells
   * outside the viewport should be filtered by callers via `containsCell` /
   * `containsViewportCell`.
   */
  getRange(frame: TermyFrame): SelectionRange | null {
    if (!this.state) return null
    const { mode, absolute } = this.state
    let anchorRow = this.state.anchorRow
    let headRow = this.state.headRow
    if (absolute) {
      anchorRow = absoluteToViewport(frame, anchorRow)
      headRow = absoluteToViewport(frame, headRow)
    }
    let startRow = anchorRow
    let startCol = this.state.anchorCol
    let endRow = headRow
    let endCol = this.state.headCol

    if (mode === 'rect') {
      // Rectangular: normalize as a bounding box. Row/col are independent.
      if (endRow < startRow) [startRow, endRow] = [endRow, startRow]
      if (endCol < startCol) [startCol, endCol] = [endCol, startCol]
    } else {
      // Linear: normalize in reading order (row-major).
      if (endRow < startRow || (endRow === startRow && endCol < startCol)) {
        ;[startRow, endRow] = [endRow, startRow]
        ;[startCol, endCol] = [endCol, startCol]
      }
      if (mode === 'word') {
        // Word expansion only makes sense on rows currently in the viewport.
        // Off-screen rows fall back to the raw column.
        if (startRow >= 0 && startRow < frame.rows) {
          startCol = this.expandWordStart(frame, startRow, startCol)
        }
        if (endRow >= 0 && endRow < frame.rows) {
          endCol = this.expandWordEnd(frame, endRow, endCol)
        }
      } else if (mode === 'line') {
        startCol = 0
        endCol = frame.cols - 1
      }
    }

    // Clamp rows into the visible viewport. If the true startRow was above
    // the viewport, the visible top row is effectively an "intermediate"
    // line-wrap row and must select from column 0 (rect mode keeps its
    // bounding-box columns). Same idea for endRow falling below.
    if (startRow < 0) {
      startRow = 0
      if (mode !== 'rect') startCol = 0
    }
    if (endRow > frame.rows - 1) {
      endRow = frame.rows - 1
      if (mode !== 'rect') endCol = frame.cols - 1
    }
    // Fully off-screen: signal nothing-to-paint while keeping state alive
    // so scrolling back into view restores the highlight.
    if (endRow < 0 || startRow > frame.rows - 1) return null

    return { startRow, startCol, endRow, endCol }
  }

  getText(frame: TermyFrame): string {
    if (!this.state) return ''
    const mode = this.state.mode
    const range = this.getRange(frame)
    if (!range) return ''

    if (mode === 'rect') {
      const minCol = Math.min(range.startCol, range.endCol)
      const maxCol = Math.max(range.startCol, range.endCol)
      const parts: string[] = []
      for (let row = range.startRow; row <= range.endRow; row++) {
        let line = ''
        for (let col = minCol; col <= maxCol; col++) {
          if (col < 0 || col >= frame.cols) {
            line += ' '
            continue
          }
          const cell = frame.cells[row * frame.cols + col]
          if (!cell) {
            line += ' '
            continue
          }
          line += cell.renderText ? cell.char : ' '
        }
        // Don't trim: preserve column alignment for block selections.
        parts.push(line)
      }
      return parts.join('\n')
    }

    const parts: string[] = []
    for (let row = range.startRow; row <= range.endRow; row++) {
      const fromCol = row === range.startRow ? range.startCol : 0
      const toCol = row === range.endRow ? range.endCol : frame.cols - 1
      let line = ''
      for (let col = fromCol; col <= toCol; col++) {
        const cell = frame.cells[row * frame.cols + col]
        if (!cell) continue
        line += cell.renderText ? cell.char : ' '
      }
      parts.push(line.replace(/\s+$/, ''))
    }
    return parts.join('\n')
  }

  /**
   * Test whether a (row, col) is inside the (already-normalized) range.
   * `row`/`col` are viewport-local — same coordinate space as the range
   * returned by `getRange(frame)`.
   *
   * For rect mode this uses the per-row column-clamped box; for everything
   * else it uses the line-wrap convention.
   */
  containsCell(range: SelectionRange, row: number, col: number): boolean {
    if (row < range.startRow || row > range.endRow) return false
    if (this.state?.mode === 'rect') {
      const minCol = Math.min(range.startCol, range.endCol)
      const maxCol = Math.max(range.startCol, range.endCol)
      return col >= minCol && col <= maxCol
    }
    if (range.startRow === range.endRow) {
      return col >= range.startCol && col <= range.endCol
    }
    if (row === range.startRow) return col >= range.startCol
    if (row === range.endRow) return col <= range.endCol
    return true
  }

  /**
   * Convenience: resolve the current range against `frame` and test a
   * viewport cell. Returns false if there is no active selection or the
   * selection is fully off-screen.
   */
  containsViewportCell(frame: TermyFrame, viewportRow: number, viewportCol: number): boolean {
    const range = this.getRange(frame)
    if (!range) return false
    return this.containsCell(range, viewportRow, viewportCol)
  }

  private isSeparator(char: string): boolean {
    return char === '' || this.separators.has(char)
  }

  private expandWordStart(frame: TermyFrame, row: number, col: number): number {
    let c = col
    while (c > 0) {
      const cell = frame.cells[row * frame.cols + (c - 1)]
      if (!cell || this.isSeparator(cell.renderText ? cell.char : ' ')) break
      c--
    }
    return c
  }

  private expandWordEnd(frame: TermyFrame, row: number, col: number): number {
    let c = col
    while (c < frame.cols - 1) {
      const cell = frame.cells[row * frame.cols + (c + 1)]
      if (!cell || this.isSeparator(cell.renderText ? cell.char : ' ')) break
      c++
    }
    return c
  }
}

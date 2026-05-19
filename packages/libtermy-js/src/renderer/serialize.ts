import type { TermyCell, TermyColor, TermyFrame } from '../index'

const TEXT_ENCODER = new TextEncoder()

export function serializeFrameToAnsi(frame: TermyFrame): Uint8Array {
  if (frame.rows === 0 || frame.cols === 0) {
    return new Uint8Array()
  }

  const parts: string[] = []
  parts.push('\x1b[2J\x1b[H\x1b[0m')

  let activeFg: TermyColor | null = null
  let activeBg: TermyColor | null = null
  let activeBold = false

  for (let row = 0; row < frame.rows; row++) {
    if (row > 0) {
      parts.push('\r\n')
    }

    for (let col = 0; col < frame.cols; col++) {
      const cell = frame.cells[row * frame.cols + col]
      if (!cell) {
        parts.push(' ')
        continue
      }

      const fgChanged = !colorsEqual(cell.fg, activeFg)
      const bgChanged = !colorsEqual(cell.bg, activeBg)
      const boldChanged = cell.bold !== activeBold
      if (fgChanged || bgChanged || boldChanged) {
        parts.push(buildSgr(cell, fgChanged, bgChanged, boldChanged))
        activeFg = cell.fg
        activeBg = cell.bg
        activeBold = cell.bold
      }

      parts.push(cell.renderText ? cell.char : ' ')
    }
  }

  parts.push('\x1b[0m')

  if (frame.cursor) {
    parts.push(`\x1b[${frame.cursor.row + 1};${frame.cursor.col + 1}H`)
  }

  return TEXT_ENCODER.encode(parts.join(''))
}

function buildSgr(
  cell: TermyCell,
  fgChanged: boolean,
  bgChanged: boolean,
  boldChanged: boolean,
): string {
  const params: string[] = []
  if (boldChanged) {
    params.push(cell.bold ? '1' : '22')
  }
  if (fgChanged) {
    params.push(`38;2;${cell.fg.r};${cell.fg.g};${cell.fg.b}`)
  }
  if (bgChanged) {
    if (cell.usesTerminalDefaultBg) {
      params.push('49')
    } else {
      params.push(`48;2;${cell.bg.r};${cell.bg.g};${cell.bg.b}`)
    }
  }
  return `\x1b[${params.join(';')}m`
}

function colorsEqual(a: TermyColor, b: TermyColor | null): boolean {
  if (!b) return false
  return a.r === b.r && a.g === b.g && a.b === b.b && a.a === b.a
}

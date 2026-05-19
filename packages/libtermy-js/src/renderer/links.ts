import type { TermyFrame } from '../index'

export interface DetectedLink {
  uri: string
  row: number
  startCol: number
  endCol: number
  /**
   * Where the link came from. `osc8` links were emitted explicitly by the
   * terminal application via OSC8 and are considered authoritative — they take
   * precedence over `regex` links when ranges overlap. `regex` links are
   * detected by scanning rendered text for URL-shaped substrings.
   */
  source: 'regex' | 'osc8'
}

const URL_REGEX = /\b(?:https?|ftp|file):\/\/[^\s<>"'`{}|\\^[\]]+/gi

export function detectLinks(frame: TermyFrame): DetectedLink[] {
  const regexLinks: DetectedLink[] = []
  for (let row = 0; row < frame.rows; row++) {
    const line = rowText(frame, row)
    if (line.length === 0) continue
    URL_REGEX.lastIndex = 0
    let match: RegExpExecArray | null
    while ((match = URL_REGEX.exec(line))) {
      const startCol = match.index
      const trimmed = trimTrailingPunctuation(match[0])
      regexLinks.push({
        uri: trimmed,
        row,
        startCol,
        endCol: startCol + trimmed.length - 1,
        source: 'regex',
      })
    }
  }

  const osc8Links = detectOsc8Links(frame)

  // OSC8 links take precedence — drop any regex link whose range overlaps an
  // OSC8 run on the same row. Terminal apps using OSC8 know the canonical URI
  // (URL-shortened display text, query params, etc.) better than a regex.
  const filteredRegex = regexLinks.filter(
    (regex) =>
      !osc8Links.some(
        (osc8) =>
          osc8.row === regex.row &&
          osc8.startCol <= regex.endCol &&
          osc8.endCol >= regex.startCol,
      ),
  )

  return [...osc8Links, ...filteredRegex]
}

function detectOsc8Links(frame: TermyFrame): DetectedLink[] {
  const links: DetectedLink[] = []
  if (!frame.hyperlinks || frame.hyperlinks.length === 0) {
    return links
  }
  for (let row = 0; row < frame.rows; row++) {
    let runId: number | null = null
    let runStart = 0
    for (let col = 0; col < frame.cols; col++) {
      const cell = frame.cells[row * frame.cols + col]
      const id = cell?.hyperlinkId ?? null
      if (id !== runId) {
        if (runId !== null) {
          const uri = frame.hyperlinks[runId]
          if (uri !== undefined) {
            links.push({
              uri,
              row,
              startCol: runStart,
              endCol: col - 1,
              source: 'osc8',
            })
          }
        }
        runId = id
        runStart = col
      }
    }
    if (runId !== null) {
      const uri = frame.hyperlinks[runId]
      if (uri !== undefined) {
        links.push({
          uri,
          row,
          startCol: runStart,
          endCol: frame.cols - 1,
          source: 'osc8',
        })
      }
    }
  }
  return links
}

export function findLinkAt(
  links: DetectedLink[],
  row: number,
  col: number,
): DetectedLink | null {
  for (const link of links) {
    if (link.row === row && col >= link.startCol && col <= link.endCol) {
      return link
    }
  }
  return null
}

function rowText(frame: TermyFrame, row: number): string {
  let text = ''
  for (let col = 0; col < frame.cols; col++) {
    const cell = frame.cells[row * frame.cols + col]
    text += cell?.renderText ? cell.char : ' '
  }
  return text
}

function trimTrailingPunctuation(uri: string): string {
  return uri.replace(/[.,;:!?)\]}>]+$/, '')
}

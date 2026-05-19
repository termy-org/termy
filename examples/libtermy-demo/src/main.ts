import {
  createTermyRenderer,
  loadTermy,
  type LoadedTermy,
  type SelectionPayload,
  type TermyRenderer,
  type TermyRendererBackend,
  type TermySearchMatch,
} from 'libtermy.js'

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

const CONFIG_CONTENTS = `theme = nord
font_family = "JetBrains Mono"
font_size = 14
`

type Mode = 'main' | 'worker'

interface DemoState {
  mode: Mode
  backend: TermyRendererBackend
  cols: number
  rows: number
}

// ---------------------------------------------------------------------------
// DOM helpers
// ---------------------------------------------------------------------------

const $ = <T extends HTMLElement = HTMLElement>(id: string): T => {
  const el = document.getElementById(id)
  if (!el) {
    throw new Error(`missing #${id} element`)
  }
  return el as T
}

const logEl = $('log') as HTMLDivElement
const linkLogEl = $('link-log') as HTMLDivElement
const stateLineEl = $('state-line') as HTMLSpanElement
const selectionTextEl = $('selection-text') as HTMLPreElement
const headlessOutputEl = $('headless-output') as HTMLPreElement
const terminalHostEl = $('terminal-host') as HTMLDivElement
const terminalEl = $('terminal') as HTMLDivElement
const searchMatchCountEl = $('search-match-count') as HTMLElement
const searchActivePosEl = $('search-active-pos') as HTMLElement
const stateBracketedEl = $('state-bracketed') as HTMLElement

function log(message: string): void {
  const line = document.createElement('div')
  const stamp = new Date().toLocaleTimeString()
  line.textContent = `[${stamp}] ${message}`
  logEl.appendChild(line)
  logEl.scrollTop = logEl.scrollHeight
  // eslint-disable-next-line no-console
  console.log('[libtermy-demo]', message)
}

function logLink(message: string): void {
  if (linkLogEl.textContent === '(none)') {
    linkLogEl.textContent = ''
  }
  const line = document.createElement('div')
  const stamp = new Date().toLocaleTimeString()
  line.textContent = `[${stamp}] ${message}`
  linkLogEl.appendChild(line)
  linkLogEl.scrollTop = linkLogEl.scrollHeight
}

// ---------------------------------------------------------------------------
// Renderer lifecycle — a single `current` slot we tear down and rebuild
// whenever the user changes mode / backend / size.
// ---------------------------------------------------------------------------

interface CurrentRenderer {
  term: TermyRenderer
  disposers: Array<() => void>
  headlessTimer: number | null
  // Track current search state separately from the renderer because the
  // tracker is internal; we cache matches so the count UI stays in sync.
  searchMatches: TermySearchMatch[]
  activeIndex: number
}

const state: DemoState = {
  mode: 'main',
  backend: 'canvas2d',
  cols: 100,
  rows: 32,
}

let current: CurrentRenderer | null = null
let termy: LoadedTermy | null = null

function teardown(): void {
  if (!current) return
  for (const dispose of current.disposers.splice(0)) {
    try {
      dispose()
    } catch (err) {
      console.warn('disposer threw', err)
    }
  }
  if (current.headlessTimer !== null) {
    window.clearInterval(current.headlessTimer)
  }
  try {
    current.term.dispose()
  } catch (err) {
    console.warn('term.dispose() threw', err)
  }
  current = null
}

function refreshStateLine(): void {
  const backendShown = current ? current.term.backend : state.backend
  const colsShown = current ? current.term.cols : state.cols
  const rowsShown = current ? current.term.rows : state.rows
  stateLineEl.textContent =
    `mode=${state.mode}  backend=${backendShown}  size=${colsShown}x${rowsShown}`
}

async function rebuild(): Promise<void> {
  if (!termy) {
    log('loadTermy() …')
    termy = await loadTermy()
    log('WASM loaded')
  }

  teardown()

  terminalHostEl.dataset.headless = state.backend === 'headless' ? 'true' : 'false'
  headlessOutputEl.textContent = ''

  // For headless mode the renderer wants a null host; for everything else
  // we render into the #terminal div.
  const host = state.backend === 'headless' ? null : terminalEl

  let term: TermyRenderer
  try {
    term = createTermyRenderer(host, {
      termy,
      backend: state.backend,
      workerized: state.mode === 'worker',
      cols: state.cols,
      rows: state.rows,
      fontFamily: 'JetBrains Mono',
      fontSize: 14,
      cursorBlink: true,
      scrollback: 10_000,
      configContents: CONFIG_CONTENTS,
    })
  } catch (err) {
    log(`createTermyRenderer threw: ${(err as Error).message}`)
    throw err
  }

  log(
    `renderer ready — mode=${state.mode} backend=${term.backend} ${term.cols}x${term.rows}`,
  )

  const disposers: Array<() => void> = []

  // -------------------------------------------------------------------------
  // Fake PTY: echo printable input, handle some control keys locally,
  // and round-trip pastes through bracketed-paste wrappers when active.
  // -------------------------------------------------------------------------

  const decoder = new TextDecoder()
  const encoder = new TextEncoder()

  const onInputDisp = term.onInput((bytes) => {
    if (bytes.length === 1) {
      const b = bytes[0]
      if (b === 0x0d) {
        term.write(encoder.encode('\r\n'))
        return
      }
      if (b === 0x7f) {
        term.write(encoder.encode('\b \b'))
        return
      }
    }
    term.write(bytes)
    log(`pty echo: ${JSON.stringify(decoder.decode(bytes))}`)
  })
  disposers.push(() => onInputDisp.dispose())

  const onResizeDisp = term.onResize((size) => {
    log(`onResize fired: ${size.cols}x${size.rows}`)
    refreshStateLine()
  })
  disposers.push(() => onResizeDisp.dispose())

  const onSelDisp = term.onSelectionChange((sel) => {
    renderSelection(sel)
  })
  disposers.push(() => onSelDisp.dispose())

  const onLinkDisp = term.onLink((link) => {
    const mods: string[] = []
    if (link.modifiers.meta) mods.push('Meta')
    if (link.modifiers.control) mods.push('Ctrl')
    if (link.modifiers.alt) mods.push('Alt')
    if (link.modifiers.shift) mods.push('Shift')
    const modStr = mods.length > 0 ? ` (${mods.join('+')})` : ''
    log(`onLink: ${link.uri} @ row=${link.row} col=${link.startCol}-${link.endCol}${modStr}`)
    logLink(`${link.uri}${modStr}`)
    window.open(link.uri, '_blank', 'noopener')
  })
  disposers.push(() => onLinkDisp.dispose())

  // Headless tick: redraw the snapshot JSON every 250ms.
  let headlessTimer: number | null = null
  if (state.backend === 'headless') {
    const tick = () => {
      try {
        const snap = term.snapshot()
        const summary = {
          backend: term.backend,
          cols: snap.cols,
          rows: snap.rows,
          cells: snap.cells.length,
          cursor: snap.cursor,
          displayOffset: snap.displayOffset,
          historySize: snap.historySize,
          applicationCursorKeys: snap.applicationCursorKeys,
          mouseMode: snap.mouseMode,
          mouseEncoding: snap.mouseEncoding,
          bracketedPaste: snap.bracketedPaste,
          hyperlinks: snap.hyperlinks.slice(0, 10),
          firstRow: snap.cells
            .filter((c) => c.row === 0)
            .map((c) => c.char)
            .join(''),
        }
        headlessOutputEl.textContent = JSON.stringify(summary, null, 2)
      } catch (err) {
        headlessOutputEl.textContent = `snapshot() failed: ${(err as Error).message}`
      }
    }
    tick()
    headlessTimer = window.setInterval(tick, 250)
  }

  current = {
    term,
    disposers,
    headlessTimer,
    searchMatches: [],
    activeIndex: 0,
  }

  // Reset the visible search state in the sidebar.
  updateSearchCountUI()
  selectionTextEl.textContent = '(none)'
  stateBracketedEl.textContent = 'off'

  refreshStateLine()

  // Focus + replay the startup script so each rebuild gives a fresh demo.
  if (state.backend !== 'headless') {
    term.focus()
  }
  runStartupScript(term.write.bind(term))
  log('startup script complete')
}

// ---------------------------------------------------------------------------
// Selection panel
// ---------------------------------------------------------------------------

function renderSelection(sel: SelectionPayload): void {
  if (!sel.active || sel.text.length === 0) {
    selectionTextEl.textContent = '(none)'
    return
  }
  const range = `[${sel.startRow},${sel.startCol}] → [${sel.endRow},${sel.endCol}]`
  selectionTextEl.textContent = `${range}\n────\n${sel.text}`
}

// ---------------------------------------------------------------------------
// Search UI
// ---------------------------------------------------------------------------

const inputSearch = $('input-search') as HTMLInputElement

function runSearch(query: string): void {
  if (!current) return
  if (query.length === 0) {
    current.term.clearSearchHighlight()
    current.searchMatches = []
    current.activeIndex = 0
    updateSearchCountUI()
    return
  }
  const matches = current.term.searchAndHighlight(query)
  current.searchMatches = matches
  current.activeIndex = 0
  updateSearchCountUI()
  log(`searchAndHighlight(${JSON.stringify(query)}): ${matches.length} matches`)
}

function updateSearchCountUI(): void {
  const count = current?.searchMatches.length ?? 0
  searchMatchCountEl.textContent = String(count)
  if (count > 0 && current) {
    searchActivePosEl.textContent = ` (active ${current.activeIndex + 1}/${count})`
  } else {
    searchActivePosEl.textContent = ''
  }
}

// ---------------------------------------------------------------------------
// UI wiring (one-time)
// ---------------------------------------------------------------------------

function wireUi(): void {
  $<HTMLSelectElement>('sel-mode').addEventListener('change', (ev) => {
    state.mode = (ev.target as HTMLSelectElement).value as Mode
    void rebuild()
  })

  $<HTMLSelectElement>('sel-backend').addEventListener('change', (ev) => {
    state.backend = (ev.target as HTMLSelectElement).value as TermyRendererBackend
    void rebuild()
  })

  $<HTMLSelectElement>('sel-size').addEventListener('change', (ev) => {
    const value = (ev.target as HTMLSelectElement).value
    const [colsStr, rowsStr] = value.split('x')
    const cols = Number(colsStr)
    const rows = Number(rowsStr)
    if (!Number.isFinite(cols) || !Number.isFinite(rows)) return
    state.cols = cols
    state.rows = rows
    if (current) {
      // Prefer in-place resize so we don't reset scrollback / state.
      current.term.resize(cols, rows)
      log(`term.resize(${cols}, ${rows})`)
      refreshStateLine()
    } else {
      void rebuild()
    }
  })

  $('btn-rebuild').addEventListener('click', () => {
    void rebuild()
  })

  // Search ------------------------------------------------------------------

  inputSearch.addEventListener('keydown', (ev) => {
    if (ev.key === 'Enter') {
      runSearch(inputSearch.value)
    }
  })

  $('btn-find-next').addEventListener('click', () => {
    if (!current) return
    // If no active query yet, treat the current input as a new search.
    if (current.searchMatches.length === 0) {
      runSearch(inputSearch.value)
      return
    }
    const match = current.term.findNextMatch()
    if (match) {
      current.activeIndex =
        (current.activeIndex + 1) % current.searchMatches.length
      updateSearchCountUI()
      log(`findNextMatch -> row ${match.row} col ${match.startCol}`)
    } else {
      log('findNextMatch -> null')
    }
  })

  $('btn-find-prev').addEventListener('click', () => {
    if (!current) return
    if (current.searchMatches.length === 0) {
      runSearch(inputSearch.value)
      return
    }
    const match = current.term.findPreviousMatch()
    if (match) {
      current.activeIndex =
        (current.activeIndex - 1 + current.searchMatches.length) %
        current.searchMatches.length
      updateSearchCountUI()
      log(`findPreviousMatch -> row ${match.row} col ${match.startCol}`)
    } else {
      log('findPreviousMatch -> null')
    }
  })

  $('btn-find-clear').addEventListener('click', () => {
    if (!current) return
    current.term.clearSearchHighlight()
    current.searchMatches = []
    current.activeIndex = 0
    updateSearchCountUI()
    log('clearSearchHighlight()')
  })

  // Bracketed paste ---------------------------------------------------------

  let bracketed = false
  $('btn-toggle-bracketed').addEventListener('click', () => {
    if (!current) return
    bracketed = !bracketed
    current.term.write(bracketed ? '\x1b[?2004h' : '\x1b[?2004l')
    stateBracketedEl.textContent = bracketed ? 'on' : 'off'
    log(`bracketed paste -> ${bracketed ? 'ENABLED' : 'disabled'} (CSI ?2004${bracketed ? 'h' : 'l'})`)
    log(
      bracketed
        ? 'Try Cmd+V into the terminal — input arrives wrapped in CSI 200~ … CSI 201~.'
        : 'Pastes will now arrive raw (no bracketed-paste wrappers).',
    )
  })

  // Action buttons ----------------------------------------------------------

  $('btn-copy').addEventListener('click', async () => {
    if (!current) return
    try {
      await current.term.copy()
      const selected = current.term.getSelection()
      log(`copy(): ${selected.length} chars copied`)
    } catch (err) {
      log(`copy() failed: ${(err as Error).message}`)
    }
  })

  $('btn-serialize').addEventListener('click', () => {
    if (!current) return
    const bytes = current.term.serialize()
    log(`serialize(): ${bytes.byteLength} bytes`)
    // Copy into a fresh Uint8Array so the Blob ctor is happy with the
    // narrower Uint8Array<ArrayBufferLike> typing some TS versions emit.
    const copy = new Uint8Array(bytes.byteLength)
    copy.set(bytes)
    const blob = new Blob([copy.buffer], { type: 'application/octet-stream' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `termy-${Date.now()}.ansi`
    document.body.appendChild(a)
    a.click()
    a.remove()
    URL.revokeObjectURL(url)
  })

  $('btn-top').addEventListener('click', () => {
    if (!current) return
    current.term.scrollLines(99_999)
    log('scrollLines(99999)')
  })

  $('btn-bottom').addEventListener('click', () => {
    if (!current) return
    current.term.scrollToBottom()
    log('scrollToBottom()')
  })

  $('btn-replay').addEventListener('click', () => {
    if (!current) return
    log('replaying startup script…')
    runStartupScript(current.term.write.bind(current.term))
  })
}

// ---------------------------------------------------------------------------
// Bootstrap
// ---------------------------------------------------------------------------

async function main(): Promise<void> {
  wireUi()
  refreshStateLine()
  await rebuild()
  log('ready — click into the terminal to type, Alt+drag for rect selection, Cmd/Ctrl+click links')
}

// ---------------------------------------------------------------------------
// Startup script — writes ANSI test content covering every libtermy feature.
// ---------------------------------------------------------------------------

function runStartupScript(write: (data: string | Uint8Array) => unknown): void {
  const ESC = '\x1b'
  const RESET = `${ESC}[0m`
  const CSI = `${ESC}[`

  const lines: string[] = []

  lines.push(`${CSI}H${CSI}2J`)
  lines.push('')
  lines.push(`${ESC}[1;38;2;143;188;187mlibtermy.js  end-to-end demo${RESET}`)
  lines.push(
    `${ESC}[2;37mcreateTermyRenderer(host, opts)  •  fake PTY  •  every feature exercised${RESET}`,
  )
  lines.push('')

  // ---- 16-color test pattern ----------------------------------------------
  lines.push(`${ESC}[1mTheme banner (16 ANSI colors)${RESET}`)
  let row = ''
  for (let i = 0; i < 8; i++) {
    row += `${ESC}[4${i};30m  ${i}  ${RESET}`
  }
  lines.push(row)
  row = ''
  for (let i = 0; i < 8; i++) {
    row += `${ESC}[10${i};30m  ${i + 8}  ${RESET}`
  }
  lines.push(row)
  row = ''
  for (let i = 0; i < 8; i++) {
    row += `${ESC}[3${i}m  fg${i}${RESET} `
  }
  lines.push(row)
  row = ''
  for (let i = 0; i < 8; i++) {
    row += `${ESC}[9${i}m  fg${i + 8}${RESET} `
  }
  lines.push(row)
  lines.push('')

  // ---- 256-color test -----------------------------------------------------
  lines.push(`${ESC}[1m256-color palette${RESET}`)
  for (let r = 0; r < 6; r++) {
    let line = ''
    for (let g = 0; g < 6; g++) {
      for (let b = 0; b < 6; b++) {
        const color = 16 + 36 * r + 6 * g + b
        line += `${ESC}[48;5;${color}m  ${RESET}`
      }
      line += ' '
    }
    lines.push(line)
  }
  let grayLine = ''
  for (let i = 232; i < 256; i++) {
    grayLine += `${ESC}[48;5;${i}m  ${RESET}`
  }
  lines.push(grayLine)
  lines.push('')

  // ---- 24-bit RGB gradient ------------------------------------------------
  lines.push(`${ESC}[1mTrue color (24-bit RGB)${RESET}`)
  const width = 64
  for (let band = 0; band < 3; band++) {
    let line = ''
    for (let i = 0; i < width; i++) {
      const t = i / (width - 1)
      let r = 0
      let g = 0
      let b = 0
      if (band === 0) {
        r = Math.round(255 * t)
        g = Math.round(255 * (1 - t))
        b = 128
      } else if (band === 1) {
        r = 32
        g = Math.round(255 * t)
        b = Math.round(255 * (1 - t))
      } else {
        r = Math.round(255 * (1 - t))
        g = 64
        b = Math.round(255 * t)
      }
      line += `${ESC}[48;2;${r};${g};${b}m ${RESET}`
    }
    lines.push(line)
  }
  // Bonus smooth RGB rainbow on a single line.
  {
    let line = ''
    const span = 80
    for (let i = 0; i < span; i++) {
      const h = (i / span) * 360
      const { r, g, b } = hslToRgb(h, 0.85, 0.55)
      line += `${ESC}[48;2;${r};${g};${b}m ${RESET}`
    }
    lines.push(line)
  }
  lines.push('')

  // ---- Bold/regular toggling ---------------------------------------------
  lines.push(`${ESC}[1mAttributes${RESET}`)
  lines.push(
    `${ESC}[1mbold${RESET} ${ESC}[2mdim${RESET} ${ESC}[3mitalic${RESET} ` +
      `${ESC}[4munderline${RESET} ${ESC}[7mreverse${RESET} ` +
      `${ESC}[1;31mbold-red${RESET} ${ESC}[1;38;5;39mbold-truecyan${RESET}`,
  )
  lines.push('')

  // ---- CJK / wide cells ---------------------------------------------------
  lines.push(`${ESC}[1mCJK / wide cells${RESET}`)
  lines.push(`日本語テスト  こんにちは  世界  🌏  中文测试  한국어 시험`)
  lines.push(`box: │┃║▓▒░█ │ pipes`)
  lines.push('')

  // ---- Emoji line ---------------------------------------------------------
  lines.push(`${ESC}[1mEmoji (wide cells, BMP + supp.)${RESET}`)
  lines.push(`🦀🎉🚀✨💀🔥🌈🐢🦄`)
  lines.push(`combining: é à ô (école, café)`)
  lines.push('')

  // ---- URLs: OSC8 vs regex-detected --------------------------------------
  lines.push(`${ESC}[1mLinks${RESET}`)
  lines.push(`before-osc8: `)
  lines.push(
    `OSC8 anchor -> \x1b]8;;https://example.com\x1b\\OSC8 link\x1b]8;;\x1b\\ <- styled by terminal`,
  )
  lines.push(
    `regex match  -> Visit https://example.com for docs. Also see https://github.com/lassejlv/termy and http://example.org/path?q=1.`,
  )
  lines.push(`after: Cmd/Ctrl+click either to open in a new tab.`)
  lines.push('')

  // ---- Mouse modes demo ---------------------------------------------------
  // These are advisory writes — they switch the parser into a mouse mode so
  // hovering / clicking in the terminal will produce mouse reports back
  // through onInput. We log a hint instead of changing visible content.
  lines.push(`${ESC}[1mMouse protocol${RESET}`)
  lines.push(`(advisory) writing CSI ?1000;1006h to enable SGR button-event reporting`)
  lines.push(`(advisory) writing CSI ?1002;1006h to enable button-press tracking`)
  lines.push(`(advisory) writing CSI ?1003;1006h to enable any-event tracking`)
  lines.push(`then CSI ?1000l + CSI ?1006l to disable. Watch the Log panel for pty echo:`)
  lines.push('')

  // ---- Cursor movement (CSI H, etc.) --------------------------------------
  lines.push(`${ESC}[1mCursor movement (CSI H / CSI A B C D)${RESET}`)
  lines.push('cursor demo:    .   .   .   .   .')
  lines.push('')

  // ---- Scrollback filler --------------------------------------------------
  lines.push(`${ESC}[1mScrollback exercise — 5000+ varied lines${RESET}`)
  const palette = [31, 32, 33, 34, 35, 36, 91, 92, 93, 94, 95, 96]
  for (let i = 0; i < 5050; i++) {
    const color = palette[i % palette.length]
    const tag = `line-${String(i).padStart(5, '0')}`
    const noise = Math.sin(i * 0.137).toFixed(4)
    const hex = ((i * 2654435761) >>> 0).toString(16).padStart(8, '0')
    lines.push(`${ESC}[${color}m${tag}${RESET}  noise=${noise}  hash=${hex}  ${'.'.repeat(i % 12)}`)
  }

  lines.push('')
  lines.push(
    `${ESC}[1;32mAll streams complete.${RESET} Type to test the keyboard encoder ↓`,
  )
  lines.push('')
  lines.push(`${ESC}[36mlibtermy.js${RESET}:demo$ `)

  // Stream the bulk content first.
  const fullText = lines.join('\r\n')
  const CHUNK = 64 * 1024
  for (let i = 0; i < fullText.length; i += CHUNK) {
    write(fullText.slice(i, i + CHUNK))
  }

  // Cursor movement burst — exercises the parser without trashing visible
  // content (we end with a fresh prompt).
  write(`${ESC}[H${ESC}[20B${ESC}[5C${ESC}[2C`)

  // Mouse mode toggles — turn on briefly, then turn back off. Any mouse
  // motion in the terminal area while a mode is active will round-trip
  // through `onInput` and appear in the Log panel as "pty echo".
  write(`${ESC}[?1000;1006h`)
  write(`${ESC}[?1002;1006h`)
  write(`${ESC}[?1003;1006h`)
  // Leave any-event + SGR encoding active so the user can immediately see
  // mouse reports while interacting. The "Toggle bracketed paste" button
  // exercises CSI ?2004h/l separately.

  write(`\r\n${ESC}[36mlibtermy.js${RESET}:demo$ `)
}

// HSL -> RGB helper for the rainbow gradient.
function hslToRgb(h: number, s: number, l: number): { r: number; g: number; b: number } {
  const c = (1 - Math.abs(2 * l - 1)) * s
  const hp = h / 60
  const x = c * (1 - Math.abs((hp % 2) - 1))
  let r1 = 0
  let g1 = 0
  let b1 = 0
  if (hp < 1) {
    r1 = c
    g1 = x
  } else if (hp < 2) {
    r1 = x
    g1 = c
  } else if (hp < 3) {
    g1 = c
    b1 = x
  } else if (hp < 4) {
    g1 = x
    b1 = c
  } else if (hp < 5) {
    r1 = x
    b1 = c
  } else {
    r1 = c
    b1 = x
  }
  const m = l - c / 2
  return {
    r: Math.round((r1 + m) * 255),
    g: Math.round((g1 + m) * 255),
    b: Math.round((b1 + m) * 255),
  }
}

main().catch((err) => {
  log(`fatal: ${(err as Error).message}`)
  // eslint-disable-next-line no-console
  console.error(err)
})

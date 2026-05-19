import type { KeyModifiers, Keystroke, TerminalKeyboardMode } from './keyboard'
import { encodeKeystroke } from './keyboard'

export interface DomInputBindings {
  onInput(payload: Uint8Array): void
  getKeyboardMode(): TerminalKeyboardMode
  isMacOption?: () => boolean
  /**
   * Whether the terminal is currently in bracketed paste mode (DECSET ?2004).
   * When true, pasted text is wrapped with `\x1b[200~` ... `\x1b[201~` markers
   * so the receiving program can distinguish pasted input from typed input.
   * Backends should read this from `core.bracketedPaste()` (or the snapshot's
   * `bracketedPaste` field) at paste time.
   */
  isBracketedPaste?: () => boolean
}

export interface DomInputAttachOptions {
  host: HTMLElement
  bindings: DomInputBindings
}

export interface DomInputController {
  focus(): void
  blur(): void
  dispose(): void
  readonly inputEl: HTMLTextAreaElement
}

const TEXT_ENCODER = new TextEncoder()

export function attachDomInput(options: DomInputAttachOptions): DomInputController {
  const { host, bindings } = options
  const previousPosition = host.style.position
  if (!previousPosition || previousPosition === 'static') {
    host.style.position = 'relative'
  }

  const inputEl = document.createElement('textarea')
  inputEl.setAttribute('aria-label', 'Terminal input')
  inputEl.setAttribute('autocapitalize', 'off')
  inputEl.setAttribute('autocorrect', 'off')
  inputEl.setAttribute('spellcheck', 'false')
  inputEl.style.position = 'absolute'
  inputEl.style.opacity = '0'
  inputEl.style.pointerEvents = 'none'
  inputEl.style.width = '1px'
  inputEl.style.height = '1px'
  inputEl.style.left = '0'
  inputEl.style.top = '0'
  inputEl.style.padding = '0'
  inputEl.style.margin = '0'
  inputEl.style.border = '0'
  inputEl.style.outline = 'none'
  inputEl.style.resize = 'none'
  inputEl.style.overflow = 'hidden'
  inputEl.style.zIndex = '-1'
  host.appendChild(inputEl)

  let composing = false

  function handleKeyDown(event: KeyboardEvent): void {
    if (composing) {
      return
    }
    const keystroke = domEventToKeystroke(event, bindings.isMacOption?.() ?? false)
    if (!keystroke) {
      return
    }
    const bytes = encodeKeystroke(keystroke, bindings.getKeyboardMode())
    if (!bytes) {
      return
    }
    event.preventDefault()
    event.stopPropagation()
    bindings.onInput(bytes)
  }

  function handlePaste(event: ClipboardEvent): void {
    const text = event.clipboardData?.getData('text/plain') ?? ''
    if (!text) {
      return
    }
    event.preventDefault()
    // Normalize newlines: CRLF -> LF, bare CR -> LF. Pasted content from
    // Windows clipboards or some browsers can include CRLF which most shells
    // don't expect.
    const normalized = text.replace(/\r\n?/g, '\n')
    const wrapped = bindings.isBracketedPaste?.()
      ? `\x1b[200~${normalized}\x1b[201~`
      : normalized
    bindings.onInput(TEXT_ENCODER.encode(wrapped))
  }

  function handleCompositionStart(): void {
    composing = true
  }

  function handleCompositionEnd(event: CompositionEvent): void {
    composing = false
    if (event.data) {
      bindings.onInput(TEXT_ENCODER.encode(event.data))
    }
    inputEl.value = ''
  }

  function handleHostMouseDown(event: MouseEvent): void {
    if (event.button !== 0) {
      return
    }
    inputEl.focus({ preventScroll: true })
  }

  inputEl.addEventListener('keydown', handleKeyDown)
  inputEl.addEventListener('paste', handlePaste)
  inputEl.addEventListener('compositionstart', handleCompositionStart)
  inputEl.addEventListener('compositionend', handleCompositionEnd)
  host.addEventListener('mousedown', handleHostMouseDown)

  return {
    inputEl,
    focus() {
      inputEl.focus({ preventScroll: true })
    },
    blur() {
      inputEl.blur()
    },
    dispose() {
      inputEl.removeEventListener('keydown', handleKeyDown)
      inputEl.removeEventListener('paste', handlePaste)
      inputEl.removeEventListener('compositionstart', handleCompositionStart)
      inputEl.removeEventListener('compositionend', handleCompositionEnd)
      host.removeEventListener('mousedown', handleHostMouseDown)
      inputEl.remove()
      if (!previousPosition || previousPosition === 'static') {
        host.style.position = previousPosition
      }
    },
  }
}

function domEventToKeystroke(event: KeyboardEvent, macOptionIsMeta: boolean): Keystroke | null {
  const modifiers: KeyModifiers = {
    control: event.ctrlKey,
    alt: event.altKey,
    shift: event.shiftKey,
    platform: event.metaKey,
    function: false,
  }

  const named = namedKeyFromDom(event.key)
  if (named) {
    return {
      key: named,
      modifiers,
      keyChar: event.key.length === 1 ? event.key : undefined,
    }
  }

  if (event.key.length === 1) {
    const char = event.key
    if (macOptionIsMeta && event.altKey && !event.ctrlKey && !event.metaKey) {
      return { key: char, modifiers, keyChar: char }
    }
    return { key: char, modifiers, keyChar: char }
  }

  return null
}

function namedKeyFromDom(key: string): string | null {
  switch (key) {
    case 'Enter':
      return 'enter'
    case 'Tab':
      return 'tab'
    case 'Escape':
      return 'escape'
    case 'Backspace':
      return 'backspace'
    case 'Delete':
      return 'delete'
    case ' ':
      return 'space'
    case 'ArrowUp':
      return 'up'
    case 'ArrowDown':
      return 'down'
    case 'ArrowLeft':
      return 'left'
    case 'ArrowRight':
      return 'right'
    case 'Home':
      return 'home'
    case 'End':
      return 'end'
    case 'PageUp':
      return 'pageup'
    case 'PageDown':
      return 'pagedown'
    case 'F1':
      return 'f1'
    case 'F2':
      return 'f2'
    case 'F3':
      return 'f3'
    case 'F4':
      return 'f4'
    case 'F5':
      return 'f5'
    case 'F6':
      return 'f6'
    case 'F7':
      return 'f7'
    case 'F8':
      return 'f8'
    case 'F9':
      return 'f9'
    case 'F10':
      return 'f10'
    case 'F11':
      return 'f11'
    case 'F12':
      return 'f12'
    case 'Insert':
      return 'insert'
    default:
      return null
  }
}

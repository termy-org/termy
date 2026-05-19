// Port of `crates/core/src/keyboard.rs`. This module encodes JS keystrokes into
// the byte sequences a terminal expects, including the Kitty enhanced keyboard
// reporting protocol. Keep behavior in lockstep with the Rust source; the Rust
// `#[test]` fixtures are the spec.

/**
 * Pointer/keyboard modifier state for a single keystroke.
 *
 * `macOptionLayout` is a JS-only escape hatch: the Rust port uses
 * `#[cfg(target_os = "macos")]` for the Option-layout text path. In JS we have
 * no compile-time platform, so callers can plumb a boolean through to opt into
 * the macOS behavior. When omitted we feature-detect via
 * `globalThis.navigator?.platform` (`"MacIntel"`, `"Macintel"`, `"MacARM"`, ...).
 */
export interface KeyModifiers {
  control: boolean
  alt: boolean
  shift: boolean
  platform: boolean
  function: boolean
  /** Treat this keystroke as if it were generated on macOS. Optional. */
  macOptionLayout?: boolean
}

export interface Keystroke {
  key: string
  keyChar?: string
  modifiers: KeyModifiers
}

/**
 * Subset of the kitty/DECSET keyboard mode flags that affect encoding. The
 * `applicationCursorKeys` flag toggles DECCKM (SS3 cursor sequences); the four
 * `report*` flags below come from the kitty keyboard protocol progressive
 * enhancement set.
 */
export interface TerminalKeyboardMode {
  applicationCursorKeys: boolean
  disambiguateEscapeCodes: boolean
  reportEventTypes: boolean
  reportAlternateKeys: boolean
  reportAllKeysAsEsc: boolean
  reportAssociatedText: boolean
}

export type TerminalKeyEventKind = 'press' | 'repeat' | 'release'

export const EMPTY_MODIFIERS: KeyModifiers = {
  control: false,
  alt: false,
  shift: false,
  platform: false,
  function: false,
}

export const DEFAULT_KEYBOARD_MODE: TerminalKeyboardMode = {
  applicationCursorKeys: false,
  disambiguateEscapeCodes: false,
  reportEventTypes: false,
  reportAlternateKeys: false,
  reportAllKeysAsEsc: false,
  reportAssociatedText: false,
}

/** Convenience: all four enhanced-reporting flags enabled. */
export const ENHANCED_KEYBOARD_MODE: TerminalKeyboardMode = {
  applicationCursorKeys: false,
  disambiguateEscapeCodes: true,
  reportEventTypes: true,
  reportAlternateKeys: true,
  reportAllKeysAsEsc: true,
  reportAssociatedText: true,
}

const TEXT_ENCODER = new TextEncoder()

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/**
 * Encode a keystroke into bytes for the terminal. Returns `null` if no bytes
 * should be sent (e.g. a release event in legacy mode).
 *
 * @param keystroke - Keystroke (key + modifiers + optional keyChar)
 * @param mode - Keyboard mode flags (defaults to legacy)
 * @param eventKind - `'press' | 'repeat' | 'release'` (defaults to `'press'`)
 * @param promptShortcutsEnabled - Whether "natural editing" shortcuts
 *   (Ctrl+arrow word jumps on non-mac, etc.) should be honored. Defaults to
 *   `true` to match the Rust call sites.
 */
export function encodeKeystroke(
  keystroke: Keystroke,
  mode: TerminalKeyboardMode = DEFAULT_KEYBOARD_MODE,
  eventKind: TerminalKeyEventKind = 'press',
  promptShortcutsEnabled: boolean = true,
): Uint8Array | null {
  if (eventKind === 'press' || eventKind === 'repeat') {
    const special = modifiedSpecialKeystrokeInput(keystroke, promptShortcutsEnabled)
    if (special) {
      return special
    }
  }

  if (!enhancedReportingActive(mode)) {
    if (eventKind === 'release') {
      return null
    }
    return basicKeystrokeToInput(keystroke, mode)
  }

  const enhanced = enhancedKeystrokeToInput(keystroke, eventKind, mode)
  if (enhanced) {
    return enhanced
  }

  if (eventKind === 'release') {
    return null
  }
  return basicKeystrokeToInput(keystroke, mode)
}

function enhancedReportingActive(mode: TerminalKeyboardMode): boolean {
  return mode.disambiguateEscapeCodes || mode.reportEventTypes || mode.reportAllKeysAsEsc
}

// ---------------------------------------------------------------------------
// Legacy encoding (`basic_keystroke_to_input` in Rust)
// ---------------------------------------------------------------------------

function basicKeystrokeToInput(
  keystroke: Keystroke,
  mode: TerminalKeyboardMode,
): Uint8Array | null {
  const key = keystroke.key
  const modifiers = keystroke.modifiers

  const named = basicNamedKey(key, modifiers, mode)
  if (named) {
    return named
  }

  const fnBytes = legacyFunctionKeyInput(key, modifiers)
  if (fnBytes) {
    return fnBytes
  }

  if (
    modifiers.control &&
    !modifiers.platform &&
    !modifiers.function &&
    key.length === 1
  ) {
    const codePoint = key.charCodeAt(0)
    if (isAsciiAlpha(codePoint)) {
      const lower = codePoint | 0x20
      return Uint8Array.of(lower - 0x60)
    }
    // Standard ASCII control range: @, A-Z, [, \, ], ^, _
    if (codePoint >= 0x40 && codePoint <= 0x5f) {
      return Uint8Array.of(codePoint & 0x1f)
    }
  }

  if (!modifiers.control && !modifiers.platform && !modifiers.function) {
    if (keystroke.keyChar && keystroke.keyChar.length > 0) {
      // The Rust legacy path emits key_char bytes directly (no ESC prefix even
      // with alt). The TS implementation previously prepended ESC for alt; we
      // keep the Rust-faithful behavior here so kitty enhanced mode shares the
      // same fallback semantics as the Rust port.
      return TEXT_ENCODER.encode(keystroke.keyChar)
    }
    if (key.length === 1) {
      return TEXT_ENCODER.encode(key)
    }
  }

  return null
}

function basicNamedKey(
  key: string,
  modifiers: KeyModifiers,
  mode: TerminalKeyboardMode,
): Uint8Array | null {
  switch (key) {
    case 'enter':
      return Uint8Array.of(modifiers.shift ? 0x0a : 0x0d)
    case 'tab':
      if (
        modifiers.shift &&
        !modifiers.control &&
        !modifiers.alt &&
        !modifiers.platform &&
        !modifiers.function
      ) {
        return bytes('\x1b[Z')
      }
      return Uint8Array.of(0x09)
    case 'escape':
      return Uint8Array.of(0x1b)
    case 'backspace':
      return Uint8Array.of(0x7f)
    case 'delete':
      return bytes('\x1b[3~')
    case 'space':
      return Uint8Array.of(0x20)
    case 'up':
      return legacyCursorKeyInput('A', mode, modifiers)
    case 'down':
      return legacyCursorKeyInput('B', mode, modifiers)
    case 'right':
      return legacyCursorKeyInput('C', mode, modifiers)
    case 'left':
      return legacyCursorKeyInput('D', mode, modifiers)
    case 'home':
      return legacyCursorKeyInput('H', mode, modifiers)
    case 'end':
      return legacyCursorKeyInput('F', mode, modifiers)
    case 'pageup':
      return bytes('\x1b[5~')
    case 'pagedown':
      return bytes('\x1b[6~')
    default:
      return null
  }
}

function legacyCursorKeyInput(
  suffix: string,
  mode: TerminalKeyboardMode,
  modifiers: KeyModifiers,
): Uint8Array {
  // Rust legacy path only emits the SS3 variant for unmodified cursor keys.
  // Modified cursor keys keep the CSI form regardless of DECCKM state.
  const prefix = modifiersAreEmpty(modifiers) && mode.applicationCursorKeys ? '\x1bO' : '\x1b['
  return bytes(`${prefix}${suffix}`)
}

function legacyFunctionKeyInput(key: string, modifiers: KeyModifiers): Uint8Array | null {
  if (modifiers.control || modifiers.alt || modifiers.shift || modifiers.platform) {
    return null
  }
  switch (key) {
    case 'f1':
      return bytes('\x1bOP')
    case 'f2':
      return bytes('\x1bOQ')
    case 'f3':
      return bytes('\x1bOR')
    case 'f4':
      return bytes('\x1bOS')
    case 'f5':
      return bytes('\x1b[15~')
    case 'f6':
      return bytes('\x1b[17~')
    case 'f7':
      return bytes('\x1b[18~')
    case 'f8':
      return bytes('\x1b[19~')
    case 'f9':
      return bytes('\x1b[20~')
    case 'f10':
      return bytes('\x1b[21~')
    case 'f11':
      return bytes('\x1b[23~')
    case 'f12':
      return bytes('\x1b[24~')
    default:
      return null
  }
}

// ---------------------------------------------------------------------------
// "Natural editing" shortcut shim (modified_special_keystroke_input)
// ---------------------------------------------------------------------------

function modifiedSpecialKeystrokeInput(
  keystroke: Keystroke,
  promptShortcutsEnabled: boolean,
): Uint8Array | null {
  const modifiers = keystroke.modifiers
  const key = keystroke.key

  if (isMacLike(modifiers)) {
    if (isPlainAlt(modifiers)) {
      switch (key) {
        case 'left':
          return bytes('\x1bb')
        case 'right':
          return bytes('\x1bf')
        case 'backspace':
          return bytes('\x1b\x7f')
        case 'delete':
          return bytes('\x1bd')
      }
      return null
    }
    if (isPlainPlatform(modifiers)) {
      switch (key) {
        case 'left':
        case 'home':
          return Uint8Array.of(0x01)
        case 'right':
        case 'end':
          return Uint8Array.of(0x05)
        case 'backspace':
          return Uint8Array.of(0x15)
        case 'delete':
          return Uint8Array.of(0x0b)
      }
      return null
    }
    return null
  }

  if (promptShortcutsEnabled && isPlainControl(modifiers)) {
    switch (key) {
      case 'left':
        return bytes('\x1bb')
      case 'right':
        return bytes('\x1bf')
      case 'backspace':
        return Uint8Array.of(0x17)
      case 'delete':
        return bytes('\x1bd')
    }
  }
  return null
}

function isPlainAlt(modifiers: KeyModifiers): boolean {
  return (
    modifiers.alt &&
    !modifiers.control &&
    !modifiers.platform &&
    !modifiers.shift &&
    !modifiers.function
  )
}

function isPlainPlatform(modifiers: KeyModifiers): boolean {
  return (
    modifiers.platform &&
    !modifiers.control &&
    !modifiers.alt &&
    !modifiers.shift &&
    !modifiers.function
  )
}

function isPlainControl(modifiers: KeyModifiers): boolean {
  return (
    modifiers.control &&
    !modifiers.platform &&
    !modifiers.alt &&
    !modifiers.shift &&
    !modifiers.function
  )
}

// ---------------------------------------------------------------------------
// Platform detection for the macOS Option-layout path
// ---------------------------------------------------------------------------

let cachedIsMac: boolean | null = null

function detectMacPlatform(): boolean {
  if (cachedIsMac !== null) {
    return cachedIsMac
  }
  const platform =
    typeof globalThis !== 'undefined' &&
    typeof globalThis.navigator !== 'undefined' &&
    typeof globalThis.navigator.platform === 'string'
      ? globalThis.navigator.platform
      : ''
  cachedIsMac = platform.toLowerCase().startsWith('mac')
  return cachedIsMac
}

/**
 * Treat this keystroke as if it occurred on macOS. The explicit
 * `macOptionLayout` flag overrides feature detection; otherwise we sniff
 * `navigator.platform`. In Node (no `navigator`) we default to `false`.
 */
function isMacLike(modifiers: KeyModifiers): boolean {
  if (modifiers.macOptionLayout !== undefined) {
    return modifiers.macOptionLayout
  }
  return detectMacPlatform()
}

// ---------------------------------------------------------------------------
// Enhanced (kitty) encoder
// ---------------------------------------------------------------------------

function enhancedKeystrokeToInput(
  keystroke: Keystroke,
  eventKind: TerminalKeyEventKind,
  mode: TerminalKeyboardMode,
): Uint8Array | null {
  return buildSequence(keystroke, eventKind, mode)
}

function modifiersAreEmpty(modifiers: KeyModifiers): boolean {
  return (
    !modifiers.control &&
    !modifiers.alt &&
    !modifiers.shift &&
    !modifiers.platform &&
    !modifiers.function
  )
}

function shouldDisambiguateEscapeCode(key: string, modifiers: KeyModifiers): boolean {
  if (key === 'escape') {
    return true
  }
  const onlyShift =
    modifiers.shift &&
    !modifiers.control &&
    !modifiers.alt &&
    !modifiers.platform &&
    !modifiers.function
  if (modifiersAreEmpty(modifiers)) {
    return false
  }
  if (!onlyShift) {
    return true
  }
  return key === 'tab' || key === 'enter' || key === 'backspace'
}

function isBasicNamedControlKey(key: string): boolean {
  return (
    key === 'tab' || key === 'enter' || key === 'escape' || key === 'backspace' || key === 'space'
  )
}

function isModifierKey(key: string): boolean {
  return key === 'shift' || key === 'control' || key === 'alt' || key === 'super' || key === 'cmd'
}

// SequenceModifiers encoding ------------------------------------------------

const MOD_SHIFT = 1 << 0
const MOD_ALT = 1 << 1
const MOD_CONTROL = 1 << 2
const MOD_SUPER = 1 << 3

function modifiersFromKeystroke(modifiers: KeyModifiers): number {
  let encoded = 0
  if (modifiers.shift) encoded |= MOD_SHIFT
  if (modifiers.alt) encoded |= MOD_ALT
  if (modifiers.control) encoded |= MOD_CONTROL
  if (modifiers.platform) encoded |= MOD_SUPER
  return encoded
}

function encodeEscSequenceMods(value: number): number {
  return value + 1
}

function setBit(value: number, flag: number, enabled: boolean): number {
  return enabled ? value | flag : value & ~flag
}

// Named keys ---------------------------------------------------------------

type NamedSequenceKey =
  | { kind: 'oneBased'; terminator: string }
  | { kind: 'tilde'; payload: string }
  | { kind: 'kitty'; payload: string; terminator: string }

function namedSequenceKey(key: string): NamedSequenceKey | null {
  switch (key) {
    case 'pageup':
      return { kind: 'tilde', payload: '5' }
    case 'pagedown':
      return { kind: 'tilde', payload: '6' }
    case 'delete':
      return { kind: 'tilde', payload: '3' }
    case 'insert':
      return { kind: 'tilde', payload: '2' }
    case 'home':
      return { kind: 'oneBased', terminator: 'H' }
    case 'end':
      return { kind: 'oneBased', terminator: 'F' }
    case 'left':
      return { kind: 'oneBased', terminator: 'D' }
    case 'right':
      return { kind: 'oneBased', terminator: 'C' }
    case 'up':
      return { kind: 'oneBased', terminator: 'A' }
    case 'down':
      return { kind: 'oneBased', terminator: 'B' }
    case 'f1':
      return { kind: 'oneBased', terminator: 'P' }
    case 'f2':
      return { kind: 'oneBased', terminator: 'Q' }
    case 'f3':
      // F3 diverges from the legacy xterm table in kitty mode.
      return { kind: 'kitty', payload: '13', terminator: '~' }
    case 'f4':
      return { kind: 'oneBased', terminator: 'S' }
    case 'f5':
      return { kind: 'tilde', payload: '15' }
    case 'f6':
      return { kind: 'tilde', payload: '17' }
    case 'f7':
      return { kind: 'tilde', payload: '18' }
    case 'f8':
      return { kind: 'tilde', payload: '19' }
    case 'f9':
      return { kind: 'tilde', payload: '20' }
    case 'f10':
      return { kind: 'tilde', payload: '21' }
    case 'f11':
      return { kind: 'tilde', payload: '23' }
    case 'f12':
      return { kind: 'tilde', payload: '24' }
    case 'f13':
      return { kind: 'kitty', payload: '57376', terminator: 'u' }
    case 'f14':
      return { kind: 'kitty', payload: '57377', terminator: 'u' }
    case 'f15':
      return { kind: 'kitty', payload: '57378', terminator: 'u' }
    case 'f16':
      return { kind: 'kitty', payload: '57379', terminator: 'u' }
    case 'f17':
      return { kind: 'kitty', payload: '57380', terminator: 'u' }
    case 'f18':
      return { kind: 'kitty', payload: '57381', terminator: 'u' }
    case 'f19':
      return { kind: 'kitty', payload: '57382', terminator: 'u' }
    case 'f20':
      return { kind: 'kitty', payload: '57383', terminator: 'u' }
    case 'f21':
      return { kind: 'kitty', payload: '57384', terminator: 'u' }
    case 'f22':
      return { kind: 'kitty', payload: '57385', terminator: 'u' }
    case 'f23':
      return { kind: 'kitty', payload: '57386', terminator: 'u' }
    case 'f24':
      return { kind: 'kitty', payload: '57387', terminator: 'u' }
    case 'f25':
      return { kind: 'kitty', payload: '57388', terminator: 'u' }
    case 'f26':
      return { kind: 'kitty', payload: '57389', terminator: 'u' }
    case 'f27':
      return { kind: 'kitty', payload: '57390', terminator: 'u' }
    case 'f28':
      return { kind: 'kitty', payload: '57391', terminator: 'u' }
    case 'f29':
      return { kind: 'kitty', payload: '57392', terminator: 'u' }
    case 'f30':
      return { kind: 'kitty', payload: '57393', terminator: 'u' }
    case 'f31':
      return { kind: 'kitty', payload: '57394', terminator: 'u' }
    case 'f32':
      return { kind: 'kitty', payload: '57395', terminator: 'u' }
    case 'f33':
      return { kind: 'kitty', payload: '57396', terminator: 'u' }
    case 'f34':
      return { kind: 'kitty', payload: '57397', terminator: 'u' }
    case 'f35':
      return { kind: 'kitty', payload: '57398', terminator: 'u' }
    case 'scrolllock':
      return { kind: 'kitty', payload: '57359', terminator: 'u' }
    case 'printscreen':
      return { kind: 'kitty', payload: '57361', terminator: 'u' }
    case 'pause':
      return { kind: 'kitty', payload: '57362', terminator: 'u' }
    case 'menu':
      return { kind: 'kitty', payload: '57363', terminator: 'u' }
    case 'capslock':
      return { kind: 'kitty', payload: '57358', terminator: 'u' }
    case 'numlock':
      return { kind: 'kitty', payload: '57360', terminator: 'u' }
    default:
      return null
  }
}

interface SequenceBase {
  payload: string
  terminator: string
}

function namedSequenceBase(
  named: NamedSequenceKey,
  modifiersEncoded: number,
  hasAssociatedText: boolean,
  includeEventType: boolean,
): SequenceBase {
  switch (named.kind) {
    case 'oneBased': {
      const payload =
        modifiersEncoded === 0 && !hasAssociatedText && !includeEventType ? '' : '1'
      return { payload, terminator: named.terminator }
    }
    case 'tilde':
      return { payload: named.payload, terminator: '~' }
    case 'kitty':
      return { payload: named.payload, terminator: named.terminator }
  }
}

// Modifier-only / control key payloads -------------------------------------

interface ControlModResult {
  base: SequenceBase
  modifiersEncoded: number
}

function modifierOrControlSequenceBase(
  keystroke: Keystroke,
  mode: TerminalKeyboardMode,
  modifiersEncoded: number,
): ControlModResult | null {
  let payload: string
  let nextMods = modifiersEncoded
  switch (keystroke.key) {
    case 'tab':
      payload = '9'
      break
    case 'enter':
      payload = '13'
      break
    case 'escape':
      payload = '27'
      break
    case 'space':
      payload = '32'
      break
    case 'backspace':
      payload = '127'
      break
    case 'shift':
      if (!mode.reportAllKeysAsEsc) return null
      nextMods = setBit(nextMods, MOD_SHIFT, keystroke.modifiers.shift)
      payload = '57447'
      break
    case 'control':
      if (!mode.reportAllKeysAsEsc) return null
      nextMods = setBit(nextMods, MOD_CONTROL, keystroke.modifiers.control)
      payload = '57448'
      break
    case 'alt':
      if (!mode.reportAllKeysAsEsc) return null
      nextMods = setBit(nextMods, MOD_ALT, keystroke.modifiers.alt)
      payload = '57449'
      break
    case 'super':
    case 'cmd':
      if (!mode.reportAllKeysAsEsc) return null
      nextMods = setBit(nextMods, MOD_SUPER, keystroke.modifiers.platform)
      payload = '57450'
      break
    case 'capslock':
      if (!mode.reportAllKeysAsEsc) return null
      payload = '57358'
      break
    case 'numlock':
      if (!mode.reportAllKeysAsEsc) return null
      payload = '57360'
      break
    default:
      return null
  }
  return {
    base: { payload, terminator: 'u' },
    modifiersEncoded: nextMods,
  }
}

// Text key payloads --------------------------------------------------------

function textualSequenceBase(
  keystroke: Keystroke,
  mode: TerminalKeyboardMode,
  hasAssociatedText: boolean,
): SequenceBase | null {
  const chars = Array.from(keystroke.key)
  if (chars.length === 1) {
    const ch = chars[0]!
    const unshifted = unshiftedTextCharacter(keystroke, ch)
    const unicodeKeyCode = unshifted.codePointAt(0)!
    const alternateKeyCode = ch.codePointAt(0)!
    const payload =
      mode.reportAlternateKeys && alternateKeyCode !== unicodeKeyCode
        ? `${unicodeKeyCode}:${alternateKeyCode}`
        : String(unicodeKeyCode)
    return { payload, terminator: 'u' }
  }
  if (mode.reportAllKeysAsEsc && hasAssociatedText) {
    return { payload: '0', terminator: 'u' }
  }
  return null
}

function unshiftedTextCharacter(keystroke: Keystroke, ch: string): string {
  if (keystroke.modifiers.shift) {
    const unshifted = asciiShiftedSymbolBase(ch)
    if (unshifted !== null) return unshifted
    return ch.toLowerCase()
  }
  const unshifted = asciiShiftedSymbolBase(ch)
  if (unshifted !== null) return unshifted
  const code = ch.charCodeAt(0)
  if (code >= 0x41 && code <= 0x5a) {
    return String.fromCharCode(code | 0x20)
  }
  return ch
}

function asciiShiftedSymbolBase(ch: string): string | null {
  switch (ch) {
    case '!':
      return '1'
    case '@':
      return '2'
    case '#':
      return '3'
    case '$':
      return '4'
    case '%':
      return '5'
    case '^':
      return '6'
    case '&':
      return '7'
    case '*':
      return '8'
    case '(':
      return '9'
    case ')':
      return '0'
    case '_':
      return '-'
    case '+':
      return '='
    case '{':
      return '['
    case '}':
      return ']'
    case '|':
      return '\\'
    case ':':
      return ';'
    case '"':
      return "'"
    case '<':
      return ','
    case '>':
      return '.'
    case '?':
      return '/'
    case '~':
      return '`'
    default:
      return null
  }
}

// macOS Option-layout text path --------------------------------------------

function pureTextEventText(keystroke: Keystroke): string | null {
  if (!isMacLike(keystroke.modifiers)) {
    return null
  }
  const modifiers = keystroke.modifiers
  if (!modifiers.alt || modifiers.control || modifiers.platform || modifiers.function) {
    return null
  }
  const text = keystroke.keyChar
  if (!text || text.length === 0 || isControlCharacter(text) || !isAscii(text)) {
    return null
  }
  const ch = text.charAt(0)
  if (
    text !== keystroke.key ||
    asciiShiftedSymbolBase(ch) !== null ||
    ch === '[' ||
    ch === ']' ||
    ch === '\\'
  ) {
    return text
  }
  return null
}

function isControlCharacter(text: string): boolean {
  if (text.length !== 1) return false
  const code = text.charCodeAt(0)
  return code < 0x20 || (code >= 0x7f && code <= 0x9f)
}

function isAscii(text: string): boolean {
  for (let i = 0; i < text.length; i++) {
    if (text.charCodeAt(i) > 0x7f) return false
  }
  return true
}

// associated_text helper ---------------------------------------------------

function associatedText(
  keystroke: Keystroke,
  eventKind: TerminalKeyEventKind,
  mode: TerminalKeyboardMode,
): string | null {
  if (!mode.reportAllKeysAsEsc || !mode.reportAssociatedText || eventKind === 'release') {
    return null
  }
  const text = keystroke.keyChar
  if (!text || text.length === 0 || isControlCharacter(text)) {
    return null
  }
  return text
}

// SequenceBuilder ----------------------------------------------------------

function buildSequence(
  keystroke: Keystroke,
  eventKind: TerminalKeyEventKind,
  mode: TerminalKeyboardMode,
): Uint8Array | null {
  if (eventKind === 'release' && !mode.reportEventTypes) {
    return null
  }

  const pureTextEvent = pureTextEventText(keystroke) !== null
  const includeEventType =
    mode.reportEventTypes && (eventKind === 'repeat' || eventKind === 'release')

  if (
    includeEventType &&
    !mode.reportAllKeysAsEsc &&
    (keystroke.key === 'enter' || keystroke.key === 'tab' || keystroke.key === 'backspace')
  ) {
    return null
  }

  let modifiersEncoded = pureTextEvent ? 0 : modifiersFromKeystroke(keystroke.modifiers)
  const assocText = associatedText(keystroke, eventKind, mode)

  if (!shouldBuild(keystroke, eventKind, mode, pureTextEvent)) {
    return null
  }

  let base: SequenceBase | null = null

  const named = namedSequenceKey(keystroke.key)
  if (named) {
    base = namedSequenceBase(named, modifiersEncoded, assocText !== null, includeEventType)
  }

  if (!base) {
    const controlMod = modifierOrControlSequenceBase(keystroke, mode, modifiersEncoded)
    if (controlMod) {
      base = controlMod.base
      modifiersEncoded = controlMod.modifiersEncoded
    }
  }

  if (!base) {
    if (pureTextEvent) {
      base = { payload: '0', terminator: 'u' }
    } else {
      base = textualSequenceBase(keystroke, mode, assocText !== null)
    }
  }

  if (!base) {
    return null
  }

  let sequence = `\x1b[${base.payload}`
  if (includeEventType || modifiersEncoded !== 0 || assocText !== null) {
    sequence += `;${encodeEscSequenceMods(modifiersEncoded)}`
  }

  if (includeEventType) {
    const eventCode = eventKindCode(eventKind)
    sequence += `:${eventCode}`
  }

  if (assocText !== null) {
    const codepoints: number[] = []
    for (const ch of assocText) {
      codepoints.push(ch.codePointAt(0)!)
    }
    for (let i = 0; i < codepoints.length; i++) {
      sequence += i === 0 ? `;${codepoints[i]}` : `:${codepoints[i]}`
    }
  }

  sequence += base.terminator
  return TEXT_ENCODER.encode(sequence)
}

function shouldBuild(
  keystroke: Keystroke,
  eventKind: TerminalKeyEventKind,
  mode: TerminalKeyboardMode,
  pureTextEvent: boolean,
): boolean {
  if (pureTextEvent) {
    return mode.reportAllKeysAsEsc
  }
  if (mode.reportAllKeysAsEsc) {
    return true
  }
  if (eventKind === 'release') {
    return mode.reportEventTypes
  }
  if (namedSequenceKey(keystroke.key) !== null) {
    return true
  }
  if (isModifierKey(keystroke.key)) {
    return false
  }
  if (mode.disambiguateEscapeCodes && shouldDisambiguateEscapeCode(keystroke.key, keystroke.modifiers)) {
    return true
  }
  if (isBasicNamedControlKey(keystroke.key)) {
    return false
  }
  const keyChars = Array.from(keystroke.key)
  const hasPlainText =
    (keystroke.keyChar !== undefined && keystroke.keyChar.length > 0) ||
    (keyChars.length === 1 &&
      !keystroke.modifiers.control &&
      !keystroke.modifiers.alt &&
      !keystroke.modifiers.platform &&
      !keystroke.modifiers.function)
  return !hasPlainText
}

// ---------------------------------------------------------------------------
// Misc helpers
// ---------------------------------------------------------------------------

function isAsciiAlpha(codePoint: number): boolean {
  return (codePoint >= 0x41 && codePoint <= 0x5a) || (codePoint >= 0x61 && codePoint <= 0x7a)
}

function eventKindCode(eventKind: TerminalKeyEventKind): string {
  switch (eventKind) {
    case 'press':
      return '1'
    case 'repeat':
      return '2'
    case 'release':
      return '3'
  }
}

function bytes(literal: string): Uint8Array {
  return TEXT_ENCODER.encode(literal)
}

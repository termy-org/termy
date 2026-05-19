/**
 * xterm-compatible mouse reporting helpers.
 *
 * The terminal core (Rust/WASM) owns the mouse mode state machine — it parses
 * `CSI ? 1000/1002/1003/1006/1015/1016 h/l` and exposes the current mode +
 * encoding on the frame snapshot, plus an `encodeMouseReport` method for
 * producing wire bytes. This module is the thin TypeScript layer that decides
 * when to call into the core and how to translate browser PointerEvents into
 * the byte-level arguments the core expects.
 */

/**
 * Mouse tracking modes mirrored from the Rust enum. The empty `none` value
 * indicates mouse reporting is disabled — selection / scrollback behave
 * normally.
 *
 * - `x10`: button press only (no release, no motion)
 * - `normal`: press + release
 * - `button-event`: press + release + drag (motion while a button is held)
 * - `any-event`: press + release + drag + bare motion
 */
export type MouseMode = 'none' | 'x10' | 'normal' | 'button-event' | 'any-event'

/**
 * Wire encoding used to format the mouse report.
 *
 * - `legacy`: the original X10 6-byte format `\x1b[M<button><col><row>`
 * - `sgr`: `\x1b[<{button};{col};{row}{M|m}` — recommended; supports release
 *   distinction and large coordinates.
 * - `utf8`: legacy format with 2-byte UTF-8 coordinates for values >= 95.
 * - `sgr-pixel`: SGR-encoded pixel coordinates (DEC mode 1016). Currently
 *   the renderer treats this the same as `sgr` using cell positions.
 */
export type MouseEncoding = 'legacy' | 'sgr' | 'utf8' | 'sgr-pixel'

/**
 * Logical mouse buttons reported by the renderer. `none` is used for motion
 * events that occur with no buttons held (only emitted in `any-event` mode).
 */
export type MouseButton =
  | 'left'
  | 'middle'
  | 'right'
  | 'wheel-up'
  | 'wheel-down'
  | 'wheel-left'
  | 'wheel-right'
  | 'none'

/**
 * Kind of mouse event being reported.
 *
 * - `down`: button press
 * - `up`: button release
 * - `drag`: motion with a button held
 * - `move`: bare motion (no button held; only meaningful in `any-event` mode)
 */
export type MouseEventKind = 'down' | 'up' | 'drag' | 'move'

export interface MouseModifiers {
  shift: boolean
  alt: boolean
  control: boolean
}

export interface MouseReportInput {
  button: MouseButton
  kind: MouseEventKind
  /** Zero-based cell column. */
  col: number
  /** Zero-based cell row. */
  row: number
  modifiers: MouseModifiers
}

/**
 * xterm mouse-modifier bitmask (matches the Rust encoder).
 */
export const MOUSE_MODIFIER_MASK = {
  SHIFT: 4,
  ALT: 8,
  CONTROL: 16,
} as const

/**
 * xterm-style protocol button numbers. The renderer hands these to
 * `core.encodeMouseReport(button, modifiers, col, row, kind)`.
 *
 * Buttons 32..34 are the "motion + button" codes used for drags in the
 * legacy encoding; the WASM core derives those automatically from the
 * `MouseEventKind`, so the JS side never needs to OR in the motion bit.
 */
export const MOUSE_BUTTON_CODE = {
  LEFT: 0,
  MIDDLE: 1,
  RIGHT: 2,
  /** Sent when motion is reported with no button held (any-event mode). */
  NONE: 3,
  WHEEL_UP: 64,
  WHEEL_DOWN: 65,
  WHEEL_LEFT: 66,
  WHEEL_RIGHT: 67,
} as const

/**
 * Numeric values that match the Rust `MouseEventKind` enum encoding.
 */
export const MOUSE_EVENT_KIND_CODE: Record<MouseEventKind, number> = {
  down: 0,
  up: 1,
  drag: 2,
  move: 3,
}

/**
 * Map a browser `PointerEvent.button` value to a protocol button code.
 *
 * `PointerEvent.button` values:
 *   0 = primary (left), 1 = auxiliary (middle), 2 = secondary (right),
 *   3 = back, 4 = forward.
 *
 * The "back"/"forward" buttons are not part of xterm's mouse protocol, so
 * we treat them as the `none` button which callers should filter out.
 */
export function pointerButtonToProtocol(button: number): MouseButton {
  switch (button) {
    case 0:
      return 'left'
    case 1:
      return 'middle'
    case 2:
      return 'right'
    default:
      return 'none'
  }
}

/**
 * Convert a `MouseButton` to the protocol button code consumed by
 * `core.encodeMouseReport`.
 */
export function buttonCode(button: MouseButton): number {
  switch (button) {
    case 'left':
      return MOUSE_BUTTON_CODE.LEFT
    case 'middle':
      return MOUSE_BUTTON_CODE.MIDDLE
    case 'right':
      return MOUSE_BUTTON_CODE.RIGHT
    case 'wheel-up':
      return MOUSE_BUTTON_CODE.WHEEL_UP
    case 'wheel-down':
      return MOUSE_BUTTON_CODE.WHEEL_DOWN
    case 'wheel-left':
      return MOUSE_BUTTON_CODE.WHEEL_LEFT
    case 'wheel-right':
      return MOUSE_BUTTON_CODE.WHEEL_RIGHT
    default:
      return MOUSE_BUTTON_CODE.NONE
  }
}

/**
 * Pack a `MouseModifiers` value into the xterm modifier bitmask.
 */
export function modifierBitmask(modifiers: MouseModifiers): number {
  let mask = 0
  if (modifiers.shift) mask |= MOUSE_MODIFIER_MASK.SHIFT
  if (modifiers.alt) mask |= MOUSE_MODIFIER_MASK.ALT
  if (modifiers.control) mask |= MOUSE_MODIFIER_MASK.CONTROL
  return mask
}

/**
 * Decide whether a given event kind should be reported under the active
 * mouse mode. When this returns `true` the renderer should:
 *   - encode the report and forward it as input,
 *   - suppress selection/drag bookkeeping for that event.
 *
 * When it returns `false` the renderer should fall back to the default
 * pointer behaviour (text selection, link hover, etc.).
 *
 * Note: this mirrors the Rust `event_allowed` logic but is intentionally
 * permissive — the WASM core is the authoritative gate and will return
 * `undefined` from `encodeMouseReport` for events it ultimately rejects.
 */
export function shouldReportMouseEvent(mode: MouseMode, kind: MouseEventKind): boolean {
  if (mode === 'none') return false
  if (kind === 'down') return true
  if (kind === 'up') return mode !== 'x10'
  if (kind === 'drag') return mode === 'button-event' || mode === 'any-event'
  if (kind === 'move') return mode === 'any-event'
  return false
}

/**
 * Returns whether shift was held — used to implement xterm's
 * shift-override convention: shift + click always starts a selection,
 * even when mouse reporting is enabled, so users can still copy text out
 * of an interactive TUI.
 */
export function isSelectionOverride(modifiers: { shift: boolean }): boolean {
  return modifiers.shift
}

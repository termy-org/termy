// Bell event handler shared by the canvas2d and webgl2 backends.
//
// A "bell" event is dispatched by the wasm screen whenever it processes a BEL
// byte (0x07). Depending on the configured `BellMode`, this handler either:
//
// * `'none'`     — does nothing.
// * `'visual'`   — records a flash start time so the caller can paint a
//                  decaying fullscreen overlay on subsequent frames.
// * `'audio'`    — plays a short sine-wave burst via the WebAudio API.
//
// v1 honors exactly one mode at a time. If a caller wants both, they should
// construct two handlers.

export type BellMode = 'none' | 'visual' | 'audio'

export interface BellOptions {
  mode: BellMode
}

const FLASH_DURATION_MS = 150
// 80 ms burst with a short attack so we don't click on the speaker.
const AUDIO_DURATION_S = 0.08
const AUDIO_ATTACK_S = 0.005
const AUDIO_RELEASE_S = 0.06
const AUDIO_FREQUENCY_HZ = 440
const AUDIO_GAIN = 0.15

// Minimal subset of the WebAudio API we touch. We use a local definition so
// this module can compile in environments without DOM lib types if anyone
// pulls it in standalone — at runtime the values still come from
// `window.AudioContext`.
interface MinimalAudioContext {
  readonly currentTime: number
  readonly destination: AudioDestinationNode
  createOscillator(): OscillatorNode
  createGain(): GainNode
  resume?(): Promise<void>
  close?(): Promise<void>
  state?: string
}

export class BellHandler {
  private mode: BellMode
  private flashStart = -1
  private audioCtx: MinimalAudioContext | null = null
  private audioFailed = false

  constructor(options: BellOptions) {
    this.mode = options.mode
  }

  setMode(mode: BellMode): void {
    this.mode = mode
    if (mode !== 'visual') {
      // Clear any pending flash so a mode switch to `none` or `audio`
      // immediately stops the overlay.
      this.flashStart = -1
    }
  }

  /**
   * Trigger a bell. Returns true if the caller should arrange to repaint
   * the visual flash overlay (i.e. `mode === 'visual'`). For `'audio'` and
   * `'none'` this returns false; the audio path is fired here synchronously.
   */
  trigger(): boolean {
    if (this.mode === 'visual') {
      this.flashStart = nowMs()
      return true
    }
    if (this.mode === 'audio') {
      this.playAudio()
      return false
    }
    return false
  }

  /**
   * Current visual flash intensity in [0, 1]. 0 = no flash, 1 = peak (i.e.
   * just triggered). Caller polls this each paint while `isFlashing()` is
   * true. Always returns 0 when `mode !== 'visual'`, so callers don't need
   * to special-case.
   */
  visualIntensity(now: number): number {
    if (this.mode !== 'visual' || this.flashStart < 0) return 0
    const elapsed = now - this.flashStart
    if (elapsed >= FLASH_DURATION_MS) {
      this.flashStart = -1
      return 0
    }
    if (elapsed < 0) return 1
    return Math.max(0, 1 - elapsed / FLASH_DURATION_MS)
  }

  isFlashing(now: number): boolean {
    if (this.mode !== 'visual' || this.flashStart < 0) return false
    return now - this.flashStart < FLASH_DURATION_MS
  }

  dispose(): void {
    this.flashStart = -1
    if (this.audioCtx && typeof this.audioCtx.close === 'function') {
      try {
        void this.audioCtx.close()
      } catch {
        // Closing a context that was never started is harmless; swallow.
      }
    }
    this.audioCtx = null
  }

  private playAudio(): void {
    if (this.audioFailed) return
    const ctx = this.ensureAudioContext()
    if (!ctx) return

    try {
      // Some browsers (Safari, Chrome with autoplay restrictions) start the
      // context in `suspended` state until a user gesture. We trigger a
      // resume here; if it rejects we mark the audio path as failed and fall
      // through to silence.
      if (ctx.state === 'suspended' && typeof ctx.resume === 'function') {
        void ctx.resume().catch(() => {
          this.audioFailed = true
        })
      }

      const osc = ctx.createOscillator()
      const gain = ctx.createGain()
      osc.type = 'sine'
      osc.frequency.value = AUDIO_FREQUENCY_HZ
      osc.connect(gain)
      gain.connect(ctx.destination)

      const startTime = ctx.currentTime
      gain.gain.setValueAtTime(0, startTime)
      gain.gain.linearRampToValueAtTime(AUDIO_GAIN, startTime + AUDIO_ATTACK_S)
      gain.gain.linearRampToValueAtTime(
        0,
        startTime + AUDIO_ATTACK_S + AUDIO_RELEASE_S,
      )

      osc.start(startTime)
      osc.stop(startTime + AUDIO_DURATION_S)
    } catch {
      // Older browsers throw if WebAudio is not available; swallow and
      // downgrade silently.
      this.audioFailed = true
    }
  }

  private ensureAudioContext(): MinimalAudioContext | null {
    if (this.audioCtx) return this.audioCtx
    if (typeof globalThis === 'undefined') return null
    const w = globalThis as unknown as {
      AudioContext?: new () => MinimalAudioContext
      webkitAudioContext?: new () => MinimalAudioContext
    }
    const Ctor = w.AudioContext ?? w.webkitAudioContext
    if (!Ctor) {
      this.audioFailed = true
      return null
    }
    try {
      this.audioCtx = new Ctor()
    } catch {
      this.audioFailed = true
      return null
    }
    return this.audioCtx
  }
}

function nowMs(): number {
  if (typeof performance !== 'undefined' && typeof performance.now === 'function') {
    return performance.now()
  }
  return Date.now()
}

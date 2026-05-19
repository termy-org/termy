import type { TermyRuntimeEvent } from '../index'
import type { ProgressPayload } from './types'

export function parseProgressPayload(raw: string): ProgressPayload {
  const body = raw.startsWith('4;') ? raw.slice(2) : raw
  const [stateStr, valueStr] = body.split(';')
  const stateCode = Number.parseInt(stateStr ?? '', 10)
  let state: ProgressPayload['state']
  switch (stateCode) {
    case 1:
      state = 'normal'
      break
    case 2:
      state = 'error'
      break
    case 3:
      state = 'indeterminate'
      break
    case 4:
      state = 'paused'
      break
    default:
      state = 'none'
      break
  }
  let value = Number.parseInt(valueStr ?? '', 10)
  if (!Number.isFinite(value) || Number.isNaN(value)) value = 0
  if (value < 0) value = 0
  if (value > 100) value = 100
  if (state === 'none' || state === 'indeterminate') value = 0
  return { state, value, raw }
}

export interface LifecycleEventDispatchers {
  title: Set<(title: string) => void>
  workingDirectory: Set<(uri: string) => void>
  progress: Set<(payload: ProgressPayload) => void>
  bell: Set<() => void>
  clipboardStore: Set<(text: string) => void>
}

export function dispatchTermyEvents(
  events: TermyRuntimeEvent[],
  dispatchers: LifecycleEventDispatchers,
): void {
  for (const event of events) {
    switch (event.kind) {
      case 'title': {
        const title = event.payload ?? ''
        for (const listener of dispatchers.title) listener(title)
        break
      }
      case 'working-directory': {
        const uri = event.payload ?? ''
        for (const listener of dispatchers.workingDirectory) listener(uri)
        break
      }
      case 'progress': {
        const payload = parseProgressPayload(event.payload ?? '')
        for (const listener of dispatchers.progress) listener(payload)
        break
      }
      case 'bell': {
        for (const listener of dispatchers.bell) listener()
        break
      }
      case 'clipboard-store': {
        const text = event.payload ?? ''
        for (const listener of dispatchers.clipboardStore) listener(text)
        break
      }
      default:
        break
    }
  }
}

export function createLifecycleDispatchers(): LifecycleEventDispatchers {
  return {
    title: new Set(),
    workingDirectory: new Set(),
    progress: new Set(),
    bell: new Set(),
    clipboardStore: new Set(),
  }
}

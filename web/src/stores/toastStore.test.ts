import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  MAX_TOASTS,
  TOAST_DURATIONS,
  TOAST_ENTER_DELAY_MS,
  TOAST_EXIT_MS,
  resetToastStore,
  toast,
  toastError,
  useToastStore,
} from './toastStore'

describe('toastStore', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    resetToastStore()
  })

  afterEach(() => {
    resetToastStore()
    vi.runOnlyPendingTimers()
    vi.useRealTimers()
  })

  it('toast() adds an info toast with the correct duration', () => {
    const id = toast('hello')
    const [entry] = useToastStore.getState().toasts

    expect(entry).toMatchObject({
      id,
      message: 'hello',
      type: 'info',
      phase: 'entering',
      durationMs: TOAST_DURATIONS.info,
    })

    vi.advanceTimersByTime(TOAST_ENTER_DELAY_MS)

    expect(useToastStore.getState().toasts[0].phase).toBe('visible')
  })

  it('drops the oldest toast when the queue overflows', () => {
    for (let i = 1; i <= MAX_TOASTS + 1; i += 1) {
      toast(`toast-${i}`)
    }

    expect(useToastStore.getState().toasts.map((entry) => entry.message)).toEqual([
      'toast-5',
      'toast-4',
      'toast-3',
      'toast-2',
    ])
  })

  it('dismissToast marks a toast as closing before removing it', () => {
    const id = toast('warn me', 'warn')

    useToastStore.getState().dismissToast(id)
    expect(useToastStore.getState().toasts[0].phase).toBe('closing')

    vi.advanceTimersByTime(TOAST_EXIT_MS)

    expect(useToastStore.getState().toasts).toHaveLength(0)
  })

  it('auto-dismisses success/info toasts but keeps error toasts until manual dismissal', () => {
    toast('saved', 'success')
    toastError('broken')

    vi.advanceTimersByTime(TOAST_DURATIONS.success ?? 0)
    vi.advanceTimersByTime(TOAST_EXIT_MS)

    expect(useToastStore.getState().toasts.map((entry) => entry.message)).toEqual(['broken'])
  })
})

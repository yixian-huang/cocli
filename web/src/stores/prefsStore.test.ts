import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { resetPrefsStore, getCollapsed, setCollapsed, applyPrefsFromServer } from './prefsStore'
import * as client from '@/api/client'

describe('prefsStore', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    resetPrefsStore()
  })
  afterEach(() => {
    vi.useRealTimers()
    vi.restoreAllMocks()
  })

  it('local update is immediate; PUT is debounced', () => {
    const putSpy = vi.spyOn(client.prefs, 'put').mockResolvedValue({ ok: true })

    setCollapsed('sidebar.channels', true)
    expect(getCollapsed('sidebar.channels')).toBe(true)
    expect(putSpy).not.toHaveBeenCalled()

    vi.advanceTimersByTime(499)
    expect(putSpy).not.toHaveBeenCalled()

    vi.advanceTimersByTime(1)
    expect(putSpy).toHaveBeenCalledTimes(1)
  })

  it('multiple rapid changes coalesce into a single PUT', () => {
    const putSpy = vi.spyOn(client.prefs, 'put').mockResolvedValue({ ok: true })

    setCollapsed('a', true)
    vi.advanceTimersByTime(100)
    setCollapsed('b', true)
    vi.advanceTimersByTime(100)
    setCollapsed('c', true)
    vi.advanceTimersByTime(500)

    expect(putSpy).toHaveBeenCalledTimes(1)
    const sent = putSpy.mock.calls[0][0] as { ui: { collapsed: Record<string, boolean> } }
    expect(sent.ui.collapsed).toEqual({ a: true, b: true, c: true })
  })

  it('applyPrefsFromServer replaces local state without merge', () => {
    setCollapsed('a', true)
    applyPrefsFromServer({ ui: { collapsed: { z: true } } })
    expect(getCollapsed('a')).toBe(false)
    expect(getCollapsed('z')).toBe(true)
  })

  it('PUT failure rolls back the local change', async () => {
    vi.spyOn(client.prefs, 'put').mockRejectedValue(new Error('500'))
    setCollapsed('a', true)
    vi.advanceTimersByTime(500)
    await Promise.resolve()
    await Promise.resolve()
    expect(getCollapsed('a')).toBe(false)
  })
})

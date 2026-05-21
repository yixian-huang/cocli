import { describe, it, expect, beforeEach } from 'vitest'
import { renderHook, act } from '@testing-library/react'
import { useTheme } from '../useTheme'
import { usePrefsStore, resetPrefsStore } from '@/stores/prefsStore'

beforeEach(() => {
  resetPrefsStore()
  document.documentElement.removeAttribute('data-theme')
  document.documentElement.removeAttribute('data-mode')
  document.documentElement.classList.remove('dark')
  try { localStorage.removeItem('cocli-theme') } catch { /* localStorage may not be available in test env */ }
})

describe('useTheme', () => {
  it('defaults to sandstone-light when nothing is set', () => {
    const { result } = renderHook(() => useTheme())
    expect(result.current.id).toBe('sandstone-light')
    expect(result.current.mode).toBe('light')
    expect(document.documentElement.getAttribute('data-theme')).toBe('sandstone-light')
    expect(document.documentElement.getAttribute('data-mode')).toBe('light')
    expect(document.documentElement.classList.contains('dark')).toBe(false)
  })

  it('setUserTheme writes through prefsStore and updates DOM', () => {
    const { result } = renderHook(() => useTheme())
    act(() => { result.current.setUserTheme('carbon-dark') })
    expect(usePrefsStore.getState().prefs.ui?.theme).toBe('carbon-dark')
    expect(document.documentElement.getAttribute('data-theme')).toBe('carbon-dark')
    expect(document.documentElement.getAttribute('data-mode')).toBe('dark')
  })

  it('toggleFamilyMode flips light↔dark within same family', () => {
    const { result } = renderHook(() => useTheme())
    expect(result.current.canToggleFamilyMode).toBe(true)
    act(() => { result.current.toggleFamilyMode() })
    expect(result.current.id).toBe('sandstone-dark')
    act(() => { result.current.toggleFamilyMode() })
    expect(result.current.id).toBe('sandstone-light')
  })

  it('canToggleFamilyMode is false for carbon-dark (no counterpart)', () => {
    const { result } = renderHook(() => useTheme())
    act(() => { result.current.setUserTheme('carbon-dark') })
    expect(result.current.canToggleFamilyMode).toBe(false)
    act(() => { result.current.toggleFamilyMode() })
    expect(result.current.id).toBe('carbon-dark')
  })

  it('mirrors theme id to localStorage for inline boot script', () => {
    const { result } = renderHook(() => useTheme())
    act(() => { result.current.setUserTheme('carbon-dark') })
    expect(localStorage.getItem('cocli-theme')).toBe('carbon-dark')
  })
})

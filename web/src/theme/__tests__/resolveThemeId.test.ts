import { describe, it, expect } from 'vitest'
import { resolveThemeId } from '../useTheme'
import { DEFAULT_THEME_ID } from '../registry'

describe('resolveThemeId', () => {
  it('user override wins over zone default', () => {
    expect(resolveThemeId('carbon-dark', 'sandstone-light')).toBe('carbon-dark')
  })

  it('falls back to zone default when user pref is missing', () => {
    expect(resolveThemeId(undefined, 'carbon-dark')).toBe('carbon-dark')
  })

  it('falls back to system default when both missing', () => {
    expect(resolveThemeId(undefined, undefined)).toBe(DEFAULT_THEME_ID)
  })

  it('treats "follow-zone" as explicit zone-tracking', () => {
    expect(resolveThemeId('follow-zone', 'carbon-dark')).toBe('carbon-dark')
  })

  it('follow-zone with missing zone falls back to system default', () => {
    expect(resolveThemeId('follow-zone', undefined)).toBe(DEFAULT_THEME_ID)
  })

  it('ignores invalid user pref and falls through to zone default', () => {
    expect(resolveThemeId('mystery', 'carbon-dark')).toBe('carbon-dark')
  })

  it('ignores invalid user pref + missing zone → system default', () => {
    expect(resolveThemeId('mystery', undefined)).toBe(DEFAULT_THEME_ID)
  })
})

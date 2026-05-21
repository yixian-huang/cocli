import { describe, it, expect } from 'vitest'
import {
  THEMES,
  DEFAULT_THEME_ID,
  isValidThemeId,
  findCounterpart,
  findTheme,
} from '../registry'

describe('THEMES registry', () => {
  it('starts with the v1 set in declared order', () => {
    expect(THEMES.map((t) => t.id).slice(0, 3)).toEqual([
      'sandstone-light',
      'sandstone-dark',
      'carbon-dark',
    ])
  })

  it('includes the brand-inspired themes after v1', () => {
    const ids = THEMES.map((t) => t.id)
    expect(ids).toContain('zoom-light')
    expect(ids).toContain('zoom-dark')
    expect(ids).toContain('slack-aubergine')
    expect(ids).toContain('discord-dark')
  })

  it('every theme has a valid shape', () => {
    for (const t of THEMES) {
      expect(t.id).toMatch(/^[a-z-]+$/)
      expect(t.family).toMatch(/^[a-z]+$/)
      expect(['light', 'dark']).toContain(t.mode)
      expect(t.labelKey.startsWith('theme.')).toBe(true)
      expect(t.preview.bg).toMatch(/^#[0-9a-f]{6}$/i)
      expect(t.preview.fg).toMatch(/^#[0-9a-f]{6}$/i)
      expect(t.preview.accent).toMatch(/^#[0-9a-f]{6}$/i)
    }
  })

  it('DEFAULT_THEME_ID exists in THEMES', () => {
    expect(THEMES.some((t) => t.id === DEFAULT_THEME_ID)).toBe(true)
  })
})

describe('isValidThemeId', () => {
  it('returns true for known ids', () => {
    expect(isValidThemeId('carbon-dark')).toBe(true)
    expect(isValidThemeId('sandstone-light')).toBe(true)
  })
  it('returns false for unknown ids', () => {
    expect(isValidThemeId('nope')).toBe(false)
    expect(isValidThemeId('')).toBe(false)
  })
})

describe('findCounterpart', () => {
  it('finds the opposite-mode theme within the same family', () => {
    expect(findCounterpart('sandstone-light')?.id).toBe('sandstone-dark')
    expect(findCounterpart('sandstone-dark')?.id).toBe('sandstone-light')
  })
  it('returns undefined when no counterpart exists', () => {
    expect(findCounterpart('carbon-dark')).toBeUndefined()
  })
  it('returns undefined for unknown id', () => {
    expect(findCounterpart('nope')).toBeUndefined()
  })
})

describe('findTheme', () => {
  it('returns the matching ThemeDef', () => {
    expect(findTheme('carbon-dark')?.family).toBe('carbon')
  })
  it('returns undefined for unknown id', () => {
    expect(findTheme('nope')).toBeUndefined()
  })
})

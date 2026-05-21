import { useCallback, useEffect } from 'react'
import {
  DEFAULT_THEME_ID,
  THEMES,
  findCounterpart,
  findTheme,
  isValidThemeId,
  type ThemeDef,
} from './registry'
import { usePrefsStore } from '@/stores/prefsStore'
import { useZoneStore } from '@/stores/zoneStore'

const STORAGE_KEY = 'cocli-theme'

export function resolveThemeId(
  userPref: string | undefined,
  zoneDefault: string | undefined,
): string {
  if (userPref === 'follow-zone') {
    return zoneDefault && isValidThemeId(zoneDefault) ? zoneDefault : DEFAULT_THEME_ID
  }
  if (userPref && isValidThemeId(userPref)) return userPref
  if (zoneDefault && isValidThemeId(zoneDefault)) return zoneDefault
  return DEFAULT_THEME_ID
}

export interface UseThemeResult {
  id: string
  def: ThemeDef
  mode: 'light' | 'dark'
  themes: readonly ThemeDef[]
  setUserTheme: (next: string) => void
  toggleFamilyMode: () => void
  canToggleFamilyMode: boolean
}

export function useTheme(): UseThemeResult {
  const userPref = usePrefsStore((s) => (s.prefs.ui as { theme?: string } | undefined)?.theme)
  const zoneTheme = useZoneStore((s) => s.activeZoneThemeId ?? undefined)

  const id = resolveThemeId(userPref, zoneTheme)
  const def = findTheme(id) ?? THEMES[0]

  useEffect(() => {
    const root = document.documentElement
    root.setAttribute('data-theme', id)
    root.setAttribute('data-mode', def.mode)
    root.classList.toggle('dark', def.mode === 'dark')
    try { localStorage.setItem(STORAGE_KEY, id) } catch { /* ignore */ }
  }, [id, def.mode])

  const setUserTheme = useCallback((next: string) => {
    usePrefsStore.getState().setPath(['ui', 'theme'], next)
  }, [])

  const toggleFamilyMode = useCallback(() => {
    const cp = findCounterpart(id)
    if (cp) usePrefsStore.getState().setPath(['ui', 'theme'], cp.id)
  }, [id])

  const canToggleFamilyMode = !!findCounterpart(id)

  return { id, def, mode: def.mode, themes: THEMES, setUserTheme, toggleFamilyMode, canToggleFamilyMode }
}

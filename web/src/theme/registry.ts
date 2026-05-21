export type ThemeMode = 'light' | 'dark'

export interface ThemeDef {
  id: string
  family: string
  mode: ThemeMode
  labelKey: string
  preview: { bg: string; fg: string; accent: string }
}

export const THEMES: readonly ThemeDef[] = [
  {
    id: 'sandstone-light',
    family: 'sandstone',
    mode: 'light',
    labelKey: 'theme.sandstone.light',
    preview: { bg: '#f5f4ed', fg: '#141413', accent: '#c96442' },
  },
  {
    id: 'sandstone-dark',
    family: 'sandstone',
    mode: 'dark',
    labelKey: 'theme.sandstone.dark',
    preview: { bg: '#141413', fg: '#faf9f5', accent: '#d97757' },
  },
  {
    id: 'carbon-dark',
    family: 'carbon',
    mode: 'dark',
    labelKey: 'theme.carbon.dark',
    preview: { bg: '#050505', fg: '#f5f5f5', accent: '#c8a26a' },
  },
  {
    id: 'zoom-light',
    family: 'zoom',
    mode: 'light',
    labelKey: 'theme.zoom.light',
    preview: { bg: '#ffffff', fg: '#1a1a1a', accent: '#2d8cff' },
  },
  {
    id: 'zoom-dark',
    family: 'zoom',
    mode: 'dark',
    labelKey: 'theme.zoom.dark',
    preview: { bg: '#1f2937', fg: '#f3f4f6', accent: '#2d8cff' },
  },
  {
    id: 'slack-aubergine',
    family: 'slack',
    mode: 'dark',
    labelKey: 'theme.slack.aubergine',
    preview: { bg: '#1a0e1b', fg: '#f8f8f8', accent: '#ecb22e' },
  },
  {
    id: 'discord-dark',
    family: 'discord',
    mode: 'dark',
    labelKey: 'theme.discord.dark',
    preview: { bg: '#2f3136', fg: '#dcddde', accent: '#5865f2' },
  },
] as const

export const DEFAULT_THEME_ID = 'sandstone-light'

export function findTheme(id: string): ThemeDef | undefined {
  return THEMES.find((t) => t.id === id)
}

export function isValidThemeId(id: string): boolean {
  return !!findTheme(id)
}

export function findCounterpart(id: string): ThemeDef | undefined {
  const cur = findTheme(id)
  if (!cur) return undefined
  return THEMES.find((t) => t.family === cur.family && t.mode !== cur.mode)
}

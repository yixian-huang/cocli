import { create } from 'zustand'
import { prefs as prefsApi } from '@/api/client'

const DEBOUNCE_MS = 500

interface UIPrefs {
  collapsed?: Record<string, boolean>
  activity?: { defaultExpandLastN?: number }
  theme?: string
}

interface PrefsShape {
  ui?: UIPrefs
  [k: string]: unknown
}

interface PrefsState {
  prefs: PrefsShape
  setPath: (path: string[], value: unknown) => void
  setFromServer: (next: PrefsShape) => void
}

let flushTimer: ReturnType<typeof setTimeout> | null = null
let lastSnapshot: PrefsShape = {}

function clone<T>(v: T): T {
  return JSON.parse(JSON.stringify(v)) as T
}

export const usePrefsStore = create<PrefsState>((set, get) => ({
  prefs: {},
  setPath: (path, value) => {
    if (path.length === 0) return
    const cur = clone(get().prefs)
    let cursor: Record<string, unknown> = cur as Record<string, unknown>
    for (let i = 0; i < path.length - 1; i++) {
      const key = path[i]
      const next = cursor[key]
      if (typeof next !== 'object' || next === null) {
        cursor[key] = {}
      }
      cursor = cursor[key] as Record<string, unknown>
    }
    cursor[path[path.length - 1]] = value
    lastSnapshot = clone(get().prefs)
    set({ prefs: cur })
    schedulePut(cur)
  },
  setFromServer: (next) => {
    if (flushTimer) {
      clearTimeout(flushTimer)
      flushTimer = null
    }
    set({ prefs: next ?? {} })
  },
}))

function schedulePut(snapshot: PrefsShape) {
  if (flushTimer) clearTimeout(flushTimer)
  flushTimer = setTimeout(async () => {
    flushTimer = null
    try {
      await prefsApi.put(snapshot as Record<string, unknown>)
    } catch {
      usePrefsStore.setState({ prefs: lastSnapshot })
    }
  }, DEBOUNCE_MS)
}

export function setCollapsed(id: string, collapsed: boolean) {
  usePrefsStore.getState().setPath(['ui', 'collapsed', id], collapsed)
}

export function getCollapsed(id: string): boolean {
  return Boolean(usePrefsStore.getState().prefs.ui?.collapsed?.[id])
}

export function applyPrefsFromServer(next: PrefsShape) {
  usePrefsStore.getState().setFromServer(next)
}

export async function bootstrapPrefs() {
  try {
    const { prefs } = await prefsApi.get()
    applyPrefsFromServer(prefs as PrefsShape)
  } catch {
    /* leave defaults */
  }
}

export function resetPrefsStore() {
  if (flushTimer) {
    clearTimeout(flushTimer)
    flushTimer = null
  }
  usePrefsStore.setState({ prefs: {} })
  lastSnapshot = {}
}

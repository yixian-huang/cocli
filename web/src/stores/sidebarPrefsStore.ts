import { create } from 'zustand'
import { storageKey as brandStorageKey } from '@/brand'

const HIDDEN_DMS_KEY_PREFIX = brandStorageKey('hidden-dms:')

function storageKey(zoneId: string): string {
  return `${HIDDEN_DMS_KEY_PREFIX}${zoneId}`
}

function readHiddenDMIds(zoneId: string): Set<string> {
  try {
    if (typeof localStorage === 'undefined' || typeof localStorage.getItem !== 'function') {
      return new Set()
    }
    const raw = localStorage.getItem(storageKey(zoneId))
    if (!raw) return new Set()

    const parsed: unknown = JSON.parse(raw)
    if (!Array.isArray(parsed)) return new Set()

    return new Set(parsed.filter((id): id is string => typeof id === 'string'))
  } catch {
    return new Set()
  }
}

function persistHiddenDMIds(zoneId: string, hiddenDMIds: Set<string>) {
  try {
    if (typeof localStorage === 'undefined' || typeof localStorage.setItem !== 'function') {
      return
    }
    localStorage.setItem(storageKey(zoneId), JSON.stringify([...hiddenDMIds]))
  } catch {
    // Ignore storage failures; the in-memory preference still updates.
  }
}

interface SidebarPrefsState {
  zoneId: string | null
  hiddenDMIds: Set<string>
  setZone: (zoneId: string | null) => void
  isDMHidden: (channelId: string) => boolean
  hideDM: (channelId: string) => void
  unhideDM: (channelId: string) => void
}

export const useSidebarPrefsStore = create<SidebarPrefsState>((set, get) => ({
  zoneId: null,
  hiddenDMIds: new Set(),

  setZone: (zoneId) => {
    set({
      zoneId,
      hiddenDMIds: zoneId ? readHiddenDMIds(zoneId) : new Set(),
    })
  },

  isDMHidden: (channelId) => get().hiddenDMIds.has(channelId),

  hideDM: (channelId) => {
    const { zoneId, hiddenDMIds } = get()
    if (!zoneId) return

    const next = new Set(hiddenDMIds)
    next.add(channelId)
    persistHiddenDMIds(zoneId, next)
    set({ hiddenDMIds: next })
  },

  unhideDM: (channelId) => {
    const { zoneId, hiddenDMIds } = get()
    if (!zoneId) return

    const next = new Set(hiddenDMIds)
    next.delete(channelId)
    persistHiddenDMIds(zoneId, next)
    set({ hiddenDMIds: next })
  },
}))

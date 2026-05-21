import { create } from 'zustand'
import { storageKey as brandStorageKey } from '@/brand'

const HIDDEN_DMS_KEY = brandStorageKey('hidden-dms')

function readHiddenDMIds(): Set<string> {
  try {
    if (typeof localStorage === 'undefined' || typeof localStorage.getItem !== 'function') {
      return new Set()
    }
    const raw = localStorage.getItem(HIDDEN_DMS_KEY)
    if (!raw) return new Set()

    const parsed: unknown = JSON.parse(raw)
    if (!Array.isArray(parsed)) return new Set()

    return new Set(parsed.filter((id): id is string => typeof id === 'string'))
  } catch {
    return new Set()
  }
}

function persistHiddenDMIds(hiddenDMIds: Set<string>) {
  try {
    if (typeof localStorage === 'undefined' || typeof localStorage.setItem !== 'function') {
      return
    }
    localStorage.setItem(HIDDEN_DMS_KEY, JSON.stringify([...hiddenDMIds]))
  } catch {
    // Ignore storage failures; the in-memory preference still updates.
  }
}

interface SidebarPrefsState {
  hiddenDMIds: Set<string>
  isDMHidden: (channelId: string) => boolean
  hideDM: (channelId: string) => void
  unhideDM: (channelId: string) => void
}

export const useSidebarPrefsStore = create<SidebarPrefsState>((set, get) => ({
  hiddenDMIds: readHiddenDMIds(),

  isDMHidden: (channelId) => get().hiddenDMIds.has(channelId),

  hideDM: (channelId) => {
    const { hiddenDMIds } = get()
    const next = new Set(hiddenDMIds)
    next.add(channelId)
    persistHiddenDMIds(next)
    set({ hiddenDMIds: next })
  },

  unhideDM: (channelId) => {
    const { hiddenDMIds } = get()
    const next = new Set(hiddenDMIds)
    next.delete(channelId)
    persistHiddenDMIds(next)
    set({ hiddenDMIds: next })
  },
}))

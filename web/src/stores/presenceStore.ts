import { create } from 'zustand'
import { presence } from '@/api/client'

interface PresenceState {
  onlineIds: Set<string>
  setOnline: (userId: string) => void
  setOffline: (userId: string) => void
  isOnline: (userId: string) => boolean
  fetchPresence: () => Promise<void>
}

export const usePresenceStore = create<PresenceState>((set, get) => ({
  onlineIds: new Set(),
  setOnline: (userId) =>
    set((s) => {
      const next = new Set(s.onlineIds)
      next.add(userId)
      return { onlineIds: next }
    }),
  setOffline: (userId) =>
    set((s) => {
      const next = new Set(s.onlineIds)
      next.delete(userId)
      return { onlineIds: next }
    }),
  isOnline: (userId) => get().onlineIds.has(userId),
  fetchPresence: async () => {
    try {
      const data = await presence.list()
      set({ onlineIds: new Set(data.online) })
    } catch {
      // ignore
    }
  },
}))

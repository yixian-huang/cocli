import { create } from 'zustand'
import type { ThreadSummary } from '@/lib/types'
import { threads as threadsApi } from '@/api/client'
import { useZoneStore } from '@/stores/zoneStore'

interface ThreadInboxState {
  threads: ThreadSummary[]
  showDone: boolean
  loading: boolean
  fetchThreads: () => Promise<void>
  toggleDone: (threadId: string) => Promise<void>
  setShowDone: (show: boolean) => void
  updateThread: (id: string, partial: Partial<ThreadSummary>) => void
  getVisibleThreads: () => ThreadSummary[]
}

export const useThreadInboxStore = create<ThreadInboxState>((set, get) => ({
  threads: [],
  showDone: false,
  loading: false,

  fetchThreads: async () => {
    const zoneId = useZoneStore.getState().activeZoneId
    if (!zoneId) return
    set({ loading: true })
    try {
      const data = await threadsApi.listAll(zoneId)
      set({ threads: data.threads, loading: false })
    } catch {
      set({ loading: false })
    }
  },

  toggleDone: async (threadId) => {
    const thread = get().threads.find((t) => t.id === threadId)
    if (!thread) return
    const newDone = !thread.done
    // Optimistic update
    get().updateThread(threadId, { done: newDone })
    try {
      await threadsApi.setDone(threadId, newDone)
    } catch {
      // Rollback
      get().updateThread(threadId, { done: !newDone })
    }
  },

  setShowDone: (show) => set({ showDone: show }),

  updateThread: (id, partial) =>
    set((state) => ({
      threads: state.threads.map((t) => (t.id === id ? { ...t, ...partial } : t)),
    })),

  getVisibleThreads: () => {
    const { threads, showDone } = get()
    const filtered = showDone ? threads : threads.filter((t) => !t.done)
    return [...filtered].sort(
      (a, b) => new Date(b.lastActivityAt).getTime() - new Date(a.lastActivityAt).getTime()
    )
  },
}))

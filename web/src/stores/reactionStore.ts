import { create } from 'zustand'
import { reactions as reactionsApi, type ReactionSummary } from '@/api/client'

const EMPTY_REACTIONS: ReactionSummary[] = []
const MAX_CONCURRENT_REACTION_REQUESTS = 4

interface ReactionState {
  summariesByMessage: Map<string, ReactionSummary[]>
  loaded: Set<string>
  inflight: Set<string>
  queue: string[]
  activeRequests: number
  ensureLoaded: (messageId: string) => void
  refresh: (messageId: string) => Promise<void>
  toggleReaction: (messageId: string, emoji: string, userId: string) => Promise<void>
}

export const useReactionStore = create<ReactionState>((set, get) => {
  const runQueue = () => {
    while (true) {
      const state = get()
      if (state.activeRequests >= MAX_CONCURRENT_REACTION_REQUESTS) return
      const nextId = state.queue.find(
        (id) => !state.inflight.has(id) && !state.loaded.has(id),
      )
      if (!nextId) return

      set((s) => {
        const inflight = new Set(s.inflight)
        inflight.add(nextId)
        return {
          queue: s.queue.filter((id) => id !== nextId),
          inflight,
          activeRequests: s.activeRequests + 1,
        }
      })

      reactionsApi
        .list(nextId)
        .then((summaries) => {
          set((s) => {
            const map = new Map(s.summariesByMessage)
            map.set(nextId, summaries || EMPTY_REACTIONS)
            const loaded = new Set(s.loaded)
            loaded.add(nextId)
            const inflight = new Set(s.inflight)
            inflight.delete(nextId)
            return {
              summariesByMessage: map,
              loaded,
              inflight,
              activeRequests: Math.max(0, s.activeRequests - 1),
            }
          })
        })
        .catch(() => {
          set((s) => {
            const inflight = new Set(s.inflight)
            inflight.delete(nextId)
            return { inflight, activeRequests: Math.max(0, s.activeRequests - 1) }
          })
        })
        .finally(() => {
          runQueue()
        })
    }
  }

  const fetchAndSet = async (messageId: string) => {
    set((s) => {
      const inflight = new Set(s.inflight)
      inflight.add(messageId)
      return { inflight }
    })
    try {
      const summaries = await reactionsApi.list(messageId)
      set((s) => {
        const map = new Map(s.summariesByMessage)
        map.set(messageId, summaries || EMPTY_REACTIONS)
        const loaded = new Set(s.loaded)
        loaded.add(messageId)
        const inflight = new Set(s.inflight)
        inflight.delete(messageId)
        return { summariesByMessage: map, loaded, inflight }
      })
    } catch {
      set((s) => {
        const inflight = new Set(s.inflight)
        inflight.delete(messageId)
        return { inflight }
      })
    }
  }

  return {
    summariesByMessage: new Map(),
    loaded: new Set(),
    inflight: new Set(),
    queue: [],
    activeRequests: 0,

    ensureLoaded: (messageId) => {
      if (!messageId) return
      const state = get()
      if (
        state.loaded.has(messageId) ||
        state.inflight.has(messageId) ||
        state.queue.includes(messageId)
      ) {
        return
      }
      set((s) => ({ queue: [...s.queue, messageId] }))
      runQueue()
    },

    refresh: async (messageId) => {
      if (!messageId) return
      const state = get()
      if (state.inflight.has(messageId)) return
      await fetchAndSet(messageId)
    },

    toggleReaction: async (messageId, emoji, userId) => {
      const summaries = get().summariesByMessage.get(messageId) || EMPTY_REACTIONS
      const existing = summaries.find((s) => s.emoji === emoji)
      const hasReacted = !!existing?.userIds.includes(userId)

      set((s) => {
        const current = s.summariesByMessage.get(messageId) || EMPTY_REACTIONS
        let next: ReactionSummary[]
        if (hasReacted) {
          next = current
            .map((item) =>
              item.emoji === emoji
                ? {
                    ...item,
                    count: Math.max(0, item.count - 1),
                    userIds: item.userIds.filter((id) => id !== userId),
                  }
                : item,
            )
            .filter((item) => item.count > 0)
        } else {
          const target = current.find((item) => item.emoji === emoji)
          if (target) {
            next = current.map((item) =>
              item.emoji === emoji
                ? { ...item, count: item.count + 1, userIds: [...item.userIds, userId] }
                : item,
            )
          } else {
            next = [...current, { emoji, count: 1, userIds: [userId] }]
          }
        }

        const map = new Map(s.summariesByMessage)
        map.set(messageId, next)
        const loaded = new Set(s.loaded)
        loaded.add(messageId)
        return { summariesByMessage: map, loaded }
      })

      try {
        if (hasReacted) await reactionsApi.remove(messageId, emoji)
        else await reactionsApi.add(messageId, emoji)
      } catch {
        await get().refresh(messageId)
      }
    },
  }
})


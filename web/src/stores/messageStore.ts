import { create } from 'zustand'
import type { Message } from '@/lib/types'
import { messages as messagesApi } from '@/api/client'

export const EMPTY_MESSAGES: Message[] = []
const MAX_MESSAGES_PER_CHANNEL = 500
const BACKFILL_CONCURRENCY = 4

interface MessageState {
  messagesByChannel: Map<string, Message[]>
  hasMore: Map<string, boolean>
  latestSeq: Map<string, number>
  loading: boolean
  getMessages: (channelId: string) => Message[]
  fetchMessages: (channelId: string) => Promise<void>
  loadOlder: (channelId: string) => Promise<void>
  addMessage: (msg: Message) => void
  updateMessage: (msg: Message) => void
  sendMessage: (channelId: string, content: string) => Promise<void>
  backfillMessages: () => Promise<void>
}

export const useMessageStore = create<MessageState>((set, get) => ({
  messagesByChannel: new Map(),
  hasMore: new Map(),
  latestSeq: new Map(),
  loading: false,

  getMessages: (channelId) => get().messagesByChannel.get(channelId) ?? EMPTY_MESSAGES,

  fetchMessages: async (channelId) => {
    set({ loading: true })
    try {
      const { messages, hasMore } = await messagesApi.list(channelId, { limit: 50 })
      const maxSeq = messages.length > 0 ? Math.max(...messages.map((m) => m.seq)) : 0
      set((s) => {
        const map = new Map(s.messagesByChannel)
        map.set(channelId, messages)
        const hm = new Map(s.hasMore)
        hm.set(channelId, hasMore)
        const ls = new Map(s.latestSeq)
        if (maxSeq > (ls.get(channelId) || 0)) ls.set(channelId, maxSeq)
        return { messagesByChannel: map, hasMore: hm, latestSeq: ls, loading: false }
      })
    } catch {
      set({ loading: false })
    }
  },

  loadOlder: async (channelId) => {
    const msgs = get().messagesByChannel.get(channelId)
    if (!msgs || msgs.length === 0) return
    const oldestSeq = msgs[0].seq
    try {
      const { messages, hasMore } = await messagesApi.list(channelId, { before: oldestSeq, limit: 50 })
      set((s) => {
        const map = new Map(s.messagesByChannel)
        const existing = map.get(channelId) || []
        map.set(channelId, [...messages, ...existing])
        const hm = new Map(s.hasMore)
        hm.set(channelId, hasMore)
        return { messagesByChannel: map, hasMore: hm }
      })
    } catch {
      // ignore
    }
  },

  addMessage: (msg) => {
    if (!msg?.channelId) return
    set((s) => {
      const map = new Map(s.messagesByChannel)
      const existing = map.get(msg.channelId) || []
      // Avoid duplicates
      if (existing.some((m) => m.id === msg.id)) return s
      const updated = [...existing, msg]
      // Cap messages to prevent memory growth
      map.set(msg.channelId, updated.length > MAX_MESSAGES_PER_CHANNEL ? updated.slice(-MAX_MESSAGES_PER_CHANNEL) : updated)
      // Track latest seq
      const ls = new Map(s.latestSeq)
      if (msg.seq > (ls.get(msg.channelId) || 0)) ls.set(msg.channelId, msg.seq)
      return { messagesByChannel: map, latestSeq: ls }
    })
  },

  updateMessage: (msg) => {
    if (!msg?.channelId) return
    set((s) => {
      const map = new Map(s.messagesByChannel)
      const existing = map.get(msg.channelId)
      if (!existing) return s
      const idx = existing.findIndex((m) => m.id === msg.id)
      if (idx === -1) return s
      const updated = [...existing]
      updated[idx] = msg // new object reference for memo
      map.set(msg.channelId, updated)
      return { messagesByChannel: map }
    })
  },

  backfillMessages: async () => {
    // Only backfill channels we've already loaded messages for
    const channelIds = Array.from(get().latestSeq.keys())
    if (channelIds.length === 0) return

    let index = 0
    const worker = async () => {
      while (index < channelIds.length) {
        const channelId = channelIds[index++]
        const lastSeq = get().latestSeq.get(channelId)
        if (!lastSeq) continue
        try {
          const { messages } = await messagesApi.list(channelId, { after: lastSeq, limit: 100 })
          if (messages.length === 0) continue
          set((s) => {
            const map = new Map(s.messagesByChannel)
            const existing = map.get(channelId) || []
            // Deduplicate by ID
            const existingIds = new Set(existing.map((m) => m.id))
            const newMsgs = messages.filter((m) => !existingIds.has(m.id))
            if (newMsgs.length === 0) return s
            const merged = [...existing, ...newMsgs]
            map.set(
              channelId,
              merged.length > MAX_MESSAGES_PER_CHANNEL ? merged.slice(-MAX_MESSAGES_PER_CHANNEL) : merged,
            )
            // Update latestSeq
            const ls = new Map(s.latestSeq)
            const maxSeq = Math.max(...messages.map((m) => m.seq))
            if (maxSeq > (ls.get(channelId) || 0)) ls.set(channelId, maxSeq)
            return { messagesByChannel: map, latestSeq: ls }
          })
        } catch {
          // ignore — individual channel backfill failure shouldn't block others
        }
      }
    }

    const workers = Array.from(
      { length: Math.min(BACKFILL_CONCURRENCY, channelIds.length) },
      () => worker(),
    )
    await Promise.all(workers)
  },

  sendMessage: async (channelId, content) => {
    const msg = await messagesApi.send(channelId, content)
    get().addMessage(msg)
  },
}))

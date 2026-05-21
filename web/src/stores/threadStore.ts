import { create } from 'zustand'
import type { Channel, Message } from '@/lib/types'
import { threads as threadsApi } from '@/api/client'

interface ThreadState {
  threadChannel: Channel | null
  parentMessage: Message | null
  loading: boolean
  openThread: (parentChannelId: string, message: Message) => Promise<void>
  closeThread: () => void
}

export const useThreadStore = create<ThreadState>((set) => ({
  threadChannel: null,
  parentMessage: null,
  loading: false,

  openThread: async (parentChannelId, message) => {
    set({ loading: true, parentMessage: message })
    try {
      const channel = await threadsApi.getOrCreate(parentChannelId, message.id)
      set({ threadChannel: channel, loading: false })
    } catch {
      set({ loading: false })
    }
  },

  closeThread: () => set({ threadChannel: null, parentMessage: null }),
}))

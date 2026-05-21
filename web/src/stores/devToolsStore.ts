import { create } from 'zustand'

export interface DevToolsEvent {
  id: string
  timestamp: number
  type: string
  agentId: string
  agentName?: string
  channelId?: string
  channelName?: string
  data: Record<string, unknown>
}

interface DevToolsState {
  events: DevToolsEvent[]
  isSubscribed: boolean
  filters: {
    agentId: string | null
    eventType: string | null
    channelName: string | null
  }
  isPaused: boolean

  subscribe: () => void
  unsubscribe: () => void
  pushEvent: (event: DevToolsEvent) => void
  setFilter: (key: keyof DevToolsState['filters'], value: string | null) => void
  togglePause: () => void
  clear: () => void
  filteredEvents: () => DevToolsEvent[]
}

const MAX_EVENTS = 1000

export const useDevToolsStore = create<DevToolsState>((set, get) => ({
  events: [],
  isSubscribed: false,
  filters: { agentId: null, eventType: null, channelName: null },
  isPaused: false,

  subscribe: () => set({ isSubscribed: true }),
  unsubscribe: () => set({ isSubscribed: false }),

  pushEvent: (event) => {
    const { isSubscribed, isPaused, events } = get()
    if (!isSubscribed || isPaused) return
    const next = [...events, event]
    if (next.length > MAX_EVENTS) next.shift()
    set({ events: next })
  },

  setFilter: (key, value) =>
    set((s) => ({ filters: { ...s.filters, [key]: value } })),

  togglePause: () => set((s) => ({ isPaused: !s.isPaused })),

  clear: () => set({ events: [] }),

  filteredEvents: () => {
    const { events, filters } = get()
    return events.filter((e) => {
      if (filters.agentId && e.agentId !== filters.agentId) return false
      if (filters.eventType && e.type !== filters.eventType) return false
      if (filters.channelName && e.channelName !== filters.channelName) return false
      return true
    })
  },
}))

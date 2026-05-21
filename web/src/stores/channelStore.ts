import { create } from 'zustand'
import type { Channel } from '@/lib/types'
import { channels as channelsApi, dm as dmApi } from '@/api/client'
import { useZoneStore } from '@/stores/zoneStore'
import { defaultTitle } from '@/brand'

export interface ChannelMember {
  id: string
  memberId: string
  memberType: string
}

interface ChannelState {
  channels: Channel[]
  archivedChannels: Channel[]
  showArchived: boolean
  dmChannels: Channel[]
  activeChannelId: string | null
  loading: boolean
  membersByChannel: Record<string, ChannelMember[]>
  setActiveChannel: (id: string) => void
  fetchChannels: () => Promise<void>
  fetchArchivedChannels: () => Promise<void>
  toggleShowArchived: () => Promise<void>
  setArchived: (channelId: string, archived: boolean) => Promise<void>
  fetchDMs: () => Promise<void>
  fetchMembers: (channelId: string) => Promise<void>
  updateUnread: (channelId: string, count: number) => void
  incrementUnread: (channelId: string) => void
  clearUnread: (channelId: string) => void
  applyChannelUpdate: (patch: Partial<Channel> & { id: string }) => void
}

export const useChannelStore = create<ChannelState>((set, get) => ({
  channels: [],
  archivedChannels: [],
  showArchived: false,
  dmChannels: [],
  activeChannelId: null,
  loading: true,
  membersByChannel: {},

  setActiveChannel: (id) => set({ activeChannelId: id }),

  fetchChannels: async () => {
    const zoneId = useZoneStore.getState().activeZoneId
    if (!zoneId) return
    try {
      const channels = await channelsApi.list(zoneId)
      set({
        channels: (channels || []).filter((c) => c.type === 'channel' && !c.archived),
        loading: false,
      })
    } catch {
      set({ loading: false })
    }
  },

  fetchArchivedChannels: async () => {
    const zoneId = useZoneStore.getState().activeZoneId
    if (!zoneId) return
    try {
      const all = await channelsApi.list(zoneId, { includeArchived: true })
      set({ archivedChannels: (all || []).filter((c) => c.type === 'channel' && c.archived) })
    } catch {
      // ignore
    }
  },

  toggleShowArchived: async () => {
    const next = !get().showArchived
    set({ showArchived: next })
    if (next && get().archivedChannels.length === 0) {
      await get().fetchArchivedChannels()
    }
  },

  setArchived: async (channelId, archived) => {
    const before = get().channels
    const beforeArch = get().archivedChannels
    if (archived) {
      const moving = before.find((c) => c.id === channelId)
      if (moving) {
        set({
          channels: before.filter((c) => c.id !== channelId),
          archivedChannels: [...beforeArch, { ...moving, archived: true }],
        })
      }
    } else {
      const moving = beforeArch.find((c) => c.id === channelId)
      if (moving) {
        set({
          archivedChannels: beforeArch.filter((c) => c.id !== channelId),
          channels: [...before, { ...moving, archived: false }],
        })
      }
    }
    try {
      await channelsApi.archive(channelId, archived)
    } catch (e) {
      set({ channels: before, archivedChannels: beforeArch })
      throw e
    }
  },

  fetchDMs: async () => {
    const zoneId = useZoneStore.getState().activeZoneId
    if (!zoneId) return
    try {
      const dms = await dmApi.list(zoneId)
      set({ dmChannels: dms || [] })
    } catch {
      // ignore
    }
  },

  fetchMembers: async (channelId) => {
    try {
      const members = await channelsApi.getMembers(channelId)
      set((s) => ({
        membersByChannel: { ...s.membersByChannel, [channelId]: members || [] },
      }))
    } catch {
      // ignore
    }
  },

  updateUnread: (channelId, count) =>
    set((s) => ({
      channels: s.channels.map((c) => (c.id === channelId ? { ...c, unreadCount: count } : c)),
      dmChannels: s.dmChannels.map((c) => (c.id === channelId ? { ...c, unreadCount: count } : c)),
    })),

  incrementUnread: (channelId) =>
    set((s) => ({
      channels: s.channels.map((c) =>
        c.id === channelId ? { ...c, unreadCount: (c.unreadCount || 0) + 1 } : c
      ),
      dmChannels: s.dmChannels.map((c) =>
        c.id === channelId ? { ...c, unreadCount: (c.unreadCount || 0) + 1 } : c
      ),
    })),

  clearUnread: (channelId) => {
    set((s) => ({
      channels: s.channels.map((c) =>
        c.id === channelId ? { ...c, unreadCount: 0 } : c
      ),
      dmChannels: s.dmChannels.map((c) =>
        c.id === channelId ? { ...c, unreadCount: 0 } : c
      ),
    }))
    // Update title bar unread count
    setTimeout(() => {
      const state = useChannelStore.getState()
      const total = [...state.channels, ...state.dmChannels].reduce((sum, c) => sum + (c.unreadCount || 0), 0)
      document.title = total > 0 ? `(${total}) ${defaultTitle}` : defaultTitle
    }, 0)
  },

  applyChannelUpdate: (patch) =>
    set((s) => ({
      channels: s.channels.map((c) => (c.id === patch.id ? { ...c, ...patch } : c)),
      dmChannels: s.dmChannels.map((c) => (c.id === patch.id ? { ...c, ...patch } : c)),
    })),
}))

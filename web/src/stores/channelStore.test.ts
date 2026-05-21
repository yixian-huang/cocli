import { describe, it, expect, beforeEach } from 'vitest'
import { useChannelStore } from './channelStore'
import type { Channel } from '@/lib/types'

const makeChannel = (id: string, name: string): Channel => ({
  id,
  name,
  type: 'channel',
  createdAt: new Date().toISOString(),
  unreadCount: 0,
})

describe('channelStore', () => {
  beforeEach(() => {
    useChannelStore.setState({
      channels: [makeChannel('ch1', 'general'), makeChannel('ch2', 'random')],
      dmChannels: [],
      activeChannelId: null,
      loading: false,
    })
  })

  it('setActiveChannel sets the active ID', () => {
    useChannelStore.getState().setActiveChannel('ch1')
    expect(useChannelStore.getState().activeChannelId).toBe('ch1')
  })

  it('incrementUnread increments count for a channel', () => {
    useChannelStore.getState().incrementUnread('ch1')
    useChannelStore.getState().incrementUnread('ch1')
    const ch = useChannelStore.getState().channels.find((c) => c.id === 'ch1')
    expect(ch?.unreadCount).toBe(2)
  })

  it('clearUnread resets count to zero', () => {
    useChannelStore.getState().incrementUnread('ch1')
    useChannelStore.getState().incrementUnread('ch1')
    useChannelStore.getState().clearUnread('ch1')
    const ch = useChannelStore.getState().channels.find((c) => c.id === 'ch1')
    expect(ch?.unreadCount).toBe(0)
  })

  it('incrementUnread does not affect other channels', () => {
    useChannelStore.getState().incrementUnread('ch1')
    const ch2 = useChannelStore.getState().channels.find((c) => c.id === 'ch2')
    expect(ch2?.unreadCount).toBe(0)
  })

  it('updateUnread sets specific count', () => {
    useChannelStore.getState().updateUnread('ch2', 5)
    const ch = useChannelStore.getState().channels.find((c) => c.id === 'ch2')
    expect(ch?.unreadCount).toBe(5)
  })
})

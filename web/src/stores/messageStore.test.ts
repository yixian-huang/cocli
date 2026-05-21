import { describe, it, expect, beforeEach } from 'vitest'
import { useMessageStore } from './messageStore'

describe('messageStore', () => {
  beforeEach(() => {
    // Reset store between tests
    useMessageStore.setState({
      messagesByChannel: new Map(),
      hasMore: new Map(),
      loading: false,
    })
  })

  it('returns empty array for unknown channel', () => {
    const msgs = useMessageStore.getState().getMessages('unknown')
    expect(msgs).toEqual([])
  })

  it('addMessage adds a message to the correct channel', () => {
    const msg = {
      id: '1',
      channelId: 'ch1',
      senderType: 'user' as const,
      senderName: 'alice',
      content: 'hello',
      seq: 1,
      createdAt: new Date().toISOString(),
    }
    useMessageStore.getState().addMessage(msg)
    const msgs = useMessageStore.getState().getMessages('ch1')
    expect(msgs).toHaveLength(1)
    expect(msgs[0].content).toBe('hello')
  })

  it('addMessage deduplicates by id', () => {
    const msg = {
      id: '1',
      channelId: 'ch1',
      senderType: 'user' as const,
      senderName: 'alice',
      content: 'hello',
      seq: 1,
      createdAt: new Date().toISOString(),
    }
    useMessageStore.getState().addMessage(msg)
    useMessageStore.getState().addMessage(msg)
    expect(useMessageStore.getState().getMessages('ch1')).toHaveLength(1)
  })

  it('addMessage ignores messages without channelId', () => {
    useMessageStore.getState().addMessage(null as never)
    expect(useMessageStore.getState().messagesByChannel.size).toBe(0)
  })

  it('keeps messages separate per channel', () => {
    const msg1 = {
      id: '1',
      channelId: 'ch1',
      senderType: 'user' as const,
      senderName: 'alice',
      content: 'in ch1',
      seq: 1,
      createdAt: new Date().toISOString(),
    }
    const msg2 = {
      id: '2',
      channelId: 'ch2',
      senderType: 'user' as const,
      senderName: 'bob',
      content: 'in ch2',
      seq: 1,
      createdAt: new Date().toISOString(),
    }
    useMessageStore.getState().addMessage(msg1)
    useMessageStore.getState().addMessage(msg2)
    expect(useMessageStore.getState().getMessages('ch1')).toHaveLength(1)
    expect(useMessageStore.getState().getMessages('ch2')).toHaveLength(1)
  })
})

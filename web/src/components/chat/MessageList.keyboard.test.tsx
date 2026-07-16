import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { act, cleanup, render, waitFor } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { MessageList } from './MessageList'
import { useAgentStore } from '@/stores/agentStore'
import { useChannelStore } from '@/stores/channelStore'
import { useMessageStore } from '@/stores/messageStore'
import { useThreadInboxStore } from '@/stores/threadInboxStore'
import { useViewStore } from '@/stores/viewStore'
import { useZoneStore } from '@/stores/zoneStore'
import { useUserStore } from '@/stores/userStore'
import { resetKeyboardShortcutsForTests } from '@/hooks/useKeyboardShortcuts'
import type { Channel, Message } from '@/lib/types'

const scrollToIndex = vi.fn()

vi.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: ({ count }: { count: number }) => ({
    getVirtualItems: () => Array.from({ length: count }, (_, index) => ({ key: index, index, start: index * 80 })),
    getTotalSize: () => count * 80,
    scrollToIndex,
    measureElement: vi.fn(),
  }),
}))

vi.mock('@/api/client', async () => {
  const actual = await vi.importActual<typeof import('@/api/client')>('@/api/client')
  return {
    ...actual,
    messages: {
      ...actual.messages,
      markRead: vi.fn().mockResolvedValue(undefined),
    },
    search: {
      ...actual.search,
      messages: vi.fn().mockResolvedValue({ messages: [] }),
    },
  }
})

const channel: Channel = {
  id: 'channel-1',
  name: 'general',
  type: 'channel',
  createdAt: new Date().toISOString(),
}

const baseMessages: Message[] = [
  {
    id: 'message-1',
    channelId: channel.id,
    senderType: 'user',
    senderName: 'alice',
    content: 'first message',
    seq: 1,
    createdAt: new Date('2026-04-24T09:00:00Z').toISOString(),
  },
  {
    id: 'message-2',
    channelId: channel.id,
    senderType: 'user',
    senderName: 'bob',
    content: 'second message',
    seq: 2,
    createdAt: new Date('2026-04-24T09:01:00Z').toISOString(),
  },
]

describe('MessageList keyboard navigation', () => {
  beforeEach(() => {
    scrollToIndex.mockReset()
    useAgentStore.setState({ agents: [], loading: false, turns: {}, currentTurnEntries: {} })
    useChannelStore.setState({
      channels: [channel],
      dmChannels: [],
      activeChannelId: channel.id,
      loading: false,
      membersByChannel: {},
    })
    useMessageStore.setState({
      messagesByChannel: new Map([[channel.id, baseMessages]]),
      hasMore: new Map([[channel.id, false]]),
      latestSeq: new Map([[channel.id, 2]]),
      loading: false,
      fetchMessages: vi.fn().mockResolvedValue(undefined),
      loadOlder: vi.fn().mockResolvedValue(undefined),
    })
    useThreadInboxStore.setState({ threads: [], showDone: false, loading: false })
    useViewStore.setState({ activeAgentId: null, quotedMessage: null })
    useZoneStore.setState({ zones: [], activeZoneId: 'zone-1', loading: false })
    useUserStore.setState({ user: null, allUsers: [], loading: false })
  })

  afterEach(() => {
    cleanup()
    resetKeyboardShortcutsForTests()
  })

  it('highlights the currently selected message as navigation events arrive', async () => {
    const { container } = render(
      <MemoryRouter>
        <MessageList channelId={channel.id} />
      </MemoryRouter>,
    )

    await act(async () => {
      window.dispatchEvent(new CustomEvent('message-list:navigate', { detail: { direction: 'previous' } }))
    })

    await waitFor(() => {
      expect(container.querySelector('[data-selected="true"]')).toHaveTextContent('second message')
    })
    expect(scrollToIndex).toHaveBeenCalledWith(1, { align: 'center' })

    await act(async () => {
      window.dispatchEvent(new CustomEvent('message-list:navigate', { detail: { direction: 'previous' } }))
    })

    await waitFor(() => {
      expect(container.querySelector('[data-selected="true"]')).toHaveTextContent('first message')
    })
    expect(scrollToIndex).toHaveBeenLastCalledWith(0, { align: 'center' })
  })
})

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { MessageList } from './MessageList'
import { useAgentStore } from '@/stores/agentStore'
import { useChannelStore } from '@/stores/channelStore'
import { useMessageStore } from '@/stores/messageStore'
import { useThreadInboxStore } from '@/stores/threadInboxStore'
import { useViewStore } from '@/stores/viewStore'
import { useZoneStore } from '@/stores/zoneStore'
import type { Channel } from '@/lib/types'

vi.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: () => ({
    getVirtualItems: () => [],
    getTotalSize: () => 0,
    scrollToIndex: vi.fn(),
    measureElement: vi.fn(),
  }),
}))

const channel: Channel = {
  id: 'channel-1',
  name: 'general',
  type: 'channel',
  createdAt: new Date().toISOString(),
}

describe('MessageList loading skeleton', () => {
  beforeEach(() => {
    useAgentStore.setState({ agents: [], loading: false, turns: {}, currentTurnEntries: {} })
    useChannelStore.setState({
      channels: [channel],
      dmChannels: [],
      activeChannelId: channel.id,
      loading: false,
      membersByChannel: {},
    })
    useMessageStore.setState({
      messagesByChannel: new Map([[channel.id, []]]),
      hasMore: new Map([[channel.id, false]]),
      latestSeq: new Map([[channel.id, 0]]),
      loading: false,
      fetchMessages: vi.fn().mockResolvedValue(undefined),
      loadOlder: vi.fn().mockResolvedValue(undefined),
    })
    useThreadInboxStore.setState({ threads: [], showDone: false, loading: false })
    useViewStore.setState({ activeAgentId: null, quotedMessage: null })
    useZoneStore.setState({ zones: [], activeZoneId: 'zone-1', loading: false })
  })

  afterEach(() => {
    cleanup()
    vi.restoreAllMocks()
  })

  it('shows skeletons while loading and removes them once loading is false', () => {
    const { rerender } = render(
      <MemoryRouter>
        <MessageList channelId={channel.id} loading />
      </MemoryRouter>,
    )

    expect(screen.getByTestId('message-list-skeleton')).toBeInTheDocument()

    rerender(
      <MemoryRouter>
        <MessageList channelId={channel.id} loading={false} />
      </MemoryRouter>,
    )

    expect(screen.queryByTestId('message-list-skeleton')).not.toBeInTheDocument()
    expect(screen.getByText('Welcome to #general')).toBeInTheDocument()
  })
})

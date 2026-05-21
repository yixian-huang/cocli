import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { MessageInput } from './MessageInput'
import { useAgentStore } from '@/stores/agentStore'
import { useChannelStore } from '@/stores/channelStore'
import { useMessageStore } from '@/stores/messageStore'
import { useUserStore } from '@/stores/userStore'
import { useViewStore } from '@/stores/viewStore'

const channelId = 'channel-1'
const sendMessage = vi.fn<(...args: unknown[]) => Promise<void>>()

// R2 (2026-04-25): the sender-side urgency selector was removed. The composer
// now only takes content; priority is determined entirely by the server-side
// rule classifier. This test covers the simplified send path.
describe('MessageInput send path', () => {
  beforeEach(() => {
    sendMessage.mockReset().mockResolvedValue(undefined)
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
      callback(0)
      return 0
    })

    useChannelStore.setState({
      channels: [],
      dmChannels: [],
      activeChannelId: channelId,
      loading: false,
      membersByChannel: {},
    })
    useMessageStore.setState({ sendMessage })
    useViewStore.setState({ activeAgentId: null, quotedMessage: null })
    useAgentStore.setState({ agents: [], loading: false })
    useUserStore.setState({ allUsers: [], loading: false, user: null })
  })

  afterEach(() => {
    cleanup()
    vi.unstubAllGlobals()
  })

  it('sends content without an urgency argument', async () => {
    render(<MessageInput channelId={channelId} />)

    const textarea = screen.getByPlaceholderText('Write a message... (@ to mention)')
    fireEvent.change(textarea, { target: { value: 'hello team' } })
    fireEvent.keyDown(textarea, { key: 'Enter' })

    await waitFor(() => {
      expect(sendMessage).toHaveBeenCalledWith(channelId, 'hello team')
    })
    // Composer must not have rendered any urgency picker.
    expect(screen.queryByRole('radio', { name: 'normal' })).toBeNull()
    expect(screen.queryByRole('radio', { name: 'critical' })).toBeNull()
  })
})

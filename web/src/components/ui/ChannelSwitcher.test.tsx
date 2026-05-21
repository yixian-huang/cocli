import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { HTMLAttributes, ReactNode } from 'react'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { ChannelSwitcher } from './ChannelSwitcher'
import { useChannelStore } from '@/stores/channelStore'
import { useWorkspacePanelStore } from '@/stores/workspacePanelStore'
import { resetKeyboardShortcutsForTests } from '@/hooks/useKeyboardShortcuts'

const navigateMock = vi.fn()

vi.mock('framer-motion', () => ({
  AnimatePresence: ({ children }: { children: ReactNode }) => <>{children}</>,
  motion: {
    div: ({ children, ...props }: HTMLAttributes<HTMLDivElement>) => <div {...props}>{children}</div>,
  },
}))

vi.mock('react-router-dom', async () => {
  const actual = await vi.importActual<typeof import('react-router-dom')>('react-router-dom')
  return {
    ...actual,
    useNavigate: () => navigateMock,
  }
})

describe('ChannelSwitcher', () => {
  beforeEach(() => {
    navigateMock.mockReset()
    useChannelStore.setState({
      channels: [
        { id: 'channel-1', name: 'general', displayName: 'General', type: 'channel', createdAt: new Date().toISOString() },
        { id: 'channel-2', name: 'support', displayName: 'Support', type: 'channel', createdAt: new Date().toISOString() },
      ],
      dmChannels: [
        { id: 'dm-1', name: 'dara', displayName: 'Dara', type: 'dm', createdAt: new Date().toISOString() },
      ],
      activeChannelId: 'channel-1',
      loading: false,
      membersByChannel: {},
    })
    useWorkspacePanelStore.setState({ panel: 'history' })
  })

  afterEach(() => {
    cleanup()
    resetKeyboardShortcutsForTests()
  })

  it('selects the highlighted result with keyboard navigation', () => {
    const onClose = vi.fn()
    render(<ChannelSwitcher open onClose={onClose} />)

    const input = screen.getByLabelText('Channel switcher search')
    fireEvent.change(input, { target: { value: 'sup' } })
    fireEvent.keyDown(input, { key: 'Enter' })

    expect(navigateMock).toHaveBeenCalledWith('/channel/channel-2')
    expect(useChannelStore.getState().activeChannelId).toBe('channel-2')
    expect(useWorkspacePanelStore.getState().panel).toBe('chat')
    expect(onClose).toHaveBeenCalledTimes(1)
  })
})

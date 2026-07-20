import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { ChannelList } from './ChannelList'
import { useChannelStore } from '@/stores/channelStore'
import { useUserStore } from '@/stores/userStore'
import { ContextMenuPortal } from '@/components/ui/ContextMenu'
import type { Channel, User } from '@/lib/types'
import * as client from '@/api/client'

const navigateMock = vi.fn()

vi.mock('react-router-dom', async () => {
  const actual = await vi.importActual<typeof import('react-router-dom')>('react-router-dom')
  return {
    ...actual,
    useNavigate: () => navigateMock,
  }
})

function makeChannel(over: Partial<Channel> & Pick<Channel, 'id' | 'name'>): Channel {
  return {
    type: 'channel',
    createdAt: new Date().toISOString(),
    ...over,
  }
}

function setUser(role: User['role']) {
  useUserStore.setState({
    user: { id: 'u1', name: 'me', role, displayName: 'Me' } as User,
  })
}

describe('<ChannelList>', () => {
  beforeEach(() => {
    navigateMock.mockReset()
    setUser('member')
    useChannelStore.setState({
      channels: [makeChannel({ id: 'c1', name: 'general', displayName: 'General' })],
      archivedChannels: [],
      showArchived: false,
      activeChannelId: 'c1',
      loading: false,
    })
  })

  afterEach(() => {
    cleanup()
    vi.restoreAllMocks()
  })

  it('navigates on row click', () => {
    render(<ChannelList />)
    fireEvent.click(screen.getByText(/general/i))
    expect(navigateMock).toHaveBeenCalledWith('/channel/c1')
  })

  it('hides archived channels by default and reveals them on toggle', async () => {
    useChannelStore.setState({
      channels: [makeChannel({ id: 'c1', name: 'active', archived: false })],
      archivedChannels: [makeChannel({ id: 'c2', name: 'stale', archived: true })],
      showArchived: false,
    })
    render(
      <>
        <ContextMenuPortal />
        <ChannelList />
      </>,
    )
    expect(screen.queryByText(/stale/)).toBeNull()
    fireEvent.click(screen.getByText(/show 1 archived/i))
    expect(await screen.findByText(/stale/)).toBeInTheDocument()
  })

  it('right-click row shows the context menu with Archive option for admin', () => {
    setUser('admin')
    useChannelStore.setState({
      channels: [makeChannel({ id: 'c1', name: 'alpha', archived: false })],
      archivedChannels: [],
    })
    render(
      <>
        <ContextMenuPortal />
        <ChannelList />
      </>,
    )
    fireEvent.contextMenu(screen.getByText('alpha').closest('li')!, { clientX: 10, clientY: 10 })
    expect(screen.getByText('Archive')).toBeInTheDocument()
  })

  it('non-admin gets no Archive option in context menu', () => {
    setUser('member')
    useChannelStore.setState({
      channels: [makeChannel({ id: 'c1', name: 'alpha' })],
    })
    render(
      <>
        <ContextMenuPortal />
        <ChannelList />
      </>,
    )
    fireEvent.contextMenu(screen.getByText('alpha').closest('li')!, { clientX: 10, clientY: 10 })
    expect(screen.queryByText('Archive')).toBeNull()
    expect(screen.getByText(/mark all as read/i)).toBeInTheDocument()
  })

  it('setArchived (called from menu) optimistically moves the row to archived', async () => {
    setUser('admin')
    useChannelStore.setState({
      channels: [makeChannel({ id: 'c1', name: 'gone' })],
      archivedChannels: [],
    })
    vi.spyOn(client.channels, 'archive').mockResolvedValue({ ok: true, archived: true })
    await useChannelStore.getState().setArchived('c1', true)
    expect(useChannelStore.getState().channels).toHaveLength(0)
    expect(useChannelStore.getState().archivedChannels).toHaveLength(1)
    expect(useChannelStore.getState().archivedChannels[0]).toMatchObject({ id: 'c1', archived: true })
  })

  it('setArchived rolls back on API failure', async () => {
    setUser('admin')
    const initial = makeChannel({ id: 'c1', name: 'gone' })
    useChannelStore.setState({
      channels: [initial],
      archivedChannels: [],
    })
    vi.spyOn(client.channels, 'archive').mockRejectedValue(new Error('500'))
    await expect(useChannelStore.getState().setArchived('c1', true)).rejects.toThrow('500')
    expect(useChannelStore.getState().channels).toEqual([initial])
    expect(useChannelStore.getState().archivedChannels).toEqual([])
  })
})

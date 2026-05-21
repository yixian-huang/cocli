import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'
import { fireEvent, render, screen, waitFor, cleanup } from '@testing-library/react'
import { CreateChannelDialog } from './CreateChannelDialog'
import { useDialogStore, resetDialogStore } from '@/stores/dialogStore'
import * as client from '@/api/client'
import type { Channel } from '@/lib/types'
import { useAgentStore } from '@/stores/agentStore'
import { useUserStore } from '@/stores/userStore'

const fakeChannel: Channel = {
  id: 'c1',
  name: 'alpha-eng',
  type: 'channel',
  createdAt: new Date().toISOString(),
}

describe('<CreateChannelDialog>', () => {
  beforeEach(() => {
    resetDialogStore()
    vi.spyOn(client.channels, 'list').mockResolvedValue([])
    useAgentStore.setState({
      agents: [{ id: 'a1', name: 'agent-1', status: 'online' } as any],
      loading: false,
    } as any)
    useUserStore.setState({
      allUsers: [{ id: 'u1', name: 'user-1', displayName: 'User One' } as any],
    } as any)
  })
  afterEach(() => {
    cleanup()
    vi.restoreAllMocks()
  })

  it('opens via dialogStore.openCreateChannel and submits', async () => {
    const createSpy = vi.spyOn(client.channels, 'create').mockResolvedValue(fakeChannel)
    render(<CreateChannelDialog />)

    useDialogStore.getState().openCreateChannel({ zoneId: 'z1' })
    await waitFor(() => screen.getByLabelText(/name/i))

    fireEvent.change(screen.getByLabelText(/name/i), { target: { value: 'alpha-eng' } })
    fireEvent.click(screen.getByRole('button', { name: /^create/i }))

    await waitFor(() => expect(createSpy).toHaveBeenCalledWith('z1', 'alpha-eng', ''))
    await waitFor(() => expect(useDialogStore.getState().active).toBe(null))
  })

  it('adds selected agents and users as channel members after create', async () => {
    const createSpy = vi.spyOn(client.channels, 'create').mockResolvedValue(fakeChannel)
    const addSpy = vi.spyOn(client.channels, 'addMember').mockResolvedValue(undefined)
    render(<CreateChannelDialog />)

    useDialogStore.getState().openCreateChannel({ zoneId: 'z1' })
    await waitFor(() => screen.getByLabelText(/name/i))

    fireEvent.change(screen.getByLabelText(/name/i), { target: { value: 'alpha-eng' } })

    // Select one agent and one user
    fireEvent.click(screen.getByLabelText('@agent-1'))
    fireEvent.click(screen.getByLabelText('User One'))

    fireEvent.click(screen.getByRole('button', { name: /^create/i }))

    await waitFor(() => expect(createSpy).toHaveBeenCalledWith('z1', 'alpha-eng', ''))
    await waitFor(() => expect(addSpy).toHaveBeenCalledWith('c1', 'a1', 'agent'))
    await waitFor(() => expect(addSpy).toHaveBeenCalledWith('c1', 'u1', 'user'))
    await waitFor(() => expect(useDialogStore.getState().active).toBe(null))
  })

  it('renders inline error on failure and keeps dialog open', async () => {
    vi.spyOn(client.channels, 'create').mockRejectedValue(new Error('409: name taken'))
    render(<CreateChannelDialog />)
    useDialogStore.getState().openCreateChannel({ zoneId: 'z1' })
    await waitFor(() => screen.getByLabelText(/name/i))
    fireEvent.change(screen.getByLabelText(/name/i), { target: { value: 'dup' } })
    fireEvent.click(screen.getByRole('button', { name: /^create/i }))
    await waitFor(() => screen.getByText(/name taken/i))
    expect(useDialogStore.getState().active).toBe('createChannel')
  })
})

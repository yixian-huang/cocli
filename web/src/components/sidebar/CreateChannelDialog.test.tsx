import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'
import { fireEvent, render, screen, waitFor, cleanup } from '@testing-library/react'
import { CreateChannelDialog } from './CreateChannelDialog'
import { useDialogStore, resetDialogStore } from '@/stores/dialogStore'
import * as client from '@/api/client'
import type { Channel, Agent } from '@/lib/types'
import { useAgentStore } from '@/stores/agentStore'

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
    const mockAgent: Agent = {
      id: 'a1',
      name: 'agent-1',
      status: 'online',
      attentionState: 'idle',
      runtime: 'test-runtime',
      model: 'test-model',
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    }
    useAgentStore.setState({
      agents: [mockAgent],
      loading: false,
    })
  })
  afterEach(() => {
    cleanup()
    vi.restoreAllMocks()
  })

  it('opens via dialogStore.openCreateChannel and submits', async () => {
    const createSpy = vi.spyOn(client.channels, 'create').mockResolvedValue(fakeChannel)
    render(<CreateChannelDialog />)

    useDialogStore.getState().openCreateChannel()
    await waitFor(() => screen.getByLabelText(/name/i))

    fireEvent.change(screen.getByLabelText(/name/i), { target: { value: 'alpha-eng' } })
    fireEvent.click(screen.getByRole('button', { name: /^create/i }))

    await waitFor(() => expect(createSpy).toHaveBeenCalledWith('alpha-eng', ''))
    await waitFor(() => expect(useDialogStore.getState().active).toBe(null))
  })

  it('adds selected agents as channel members after create', async () => {
    const createSpy = vi.spyOn(client.channels, 'create').mockResolvedValue(fakeChannel)
    const addSpy = vi.spyOn(client.channels, 'addMember').mockResolvedValue(undefined)
    render(<CreateChannelDialog />)

    useDialogStore.getState().openCreateChannel()
    await waitFor(() => screen.getByLabelText(/name/i))

    fireEvent.change(screen.getByLabelText(/name/i), { target: { value: 'alpha-eng' } })

    // Select one agent
    fireEvent.click(screen.getByLabelText('@agent-1'))

    fireEvent.click(screen.getByRole('button', { name: /^create/i }))

    await waitFor(() => expect(createSpy).toHaveBeenCalledWith('alpha-eng', ''))
    await waitFor(() => expect(addSpy).toHaveBeenCalledWith('c1', 'a1', 'agent'))
    await waitFor(() => expect(useDialogStore.getState().active).toBe(null))
  })

  it('renders inline error on failure and keeps dialog open', async () => {
    vi.spyOn(client.channels, 'create').mockRejectedValue(new Error('409: name taken'))
    render(<CreateChannelDialog />)
    useDialogStore.getState().openCreateChannel()
    await waitFor(() => screen.getByLabelText(/name/i))
    fireEvent.change(screen.getByLabelText(/name/i), { target: { value: 'dup' } })
    fireEvent.click(screen.getByRole('button', { name: /^create/i }))
    await waitFor(() => screen.getByText(/name taken/i))
    expect(useDialogStore.getState().active).toBe('createChannel')
  })
})

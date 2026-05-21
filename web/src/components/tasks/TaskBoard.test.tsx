import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { TaskBoard } from './TaskBoard'
import { useChannelStore } from '@/stores/channelStore'
import { useTaskStore } from '@/stores/taskStore'

const channelId = 'channel-1'

describe('TaskBoard loading skeleton', () => {
  beforeEach(() => {
    useChannelStore.setState({
      channels: [],
      dmChannels: [],
      activeChannelId: channelId,
      loading: false,
      membersByChannel: {},
    })
    useTaskStore.setState({
      tasksByChannel: new Map([[channelId, []]]),
      dependencies: new Map(),
      loading: false,
      fetchTasks: vi.fn().mockResolvedValue(undefined),
      createTask: vi.fn().mockResolvedValue(undefined),
      updateTask: vi.fn(),
      getDeps: vi.fn().mockReturnValue([]),
    })
  })

  afterEach(() => {
    cleanup()
    vi.restoreAllMocks()
  })

  it('shows skeletons while loading and removes them once loading is false', () => {
    const { rerender } = render(<TaskBoard loading />)

    expect(screen.getByTestId('task-board-skeleton')).toBeInTheDocument()

    rerender(<TaskBoard loading={false} />)

    expect(screen.queryByTestId('task-board-skeleton')).not.toBeInTheDocument()
    expect(screen.getByText('No tasks in this channel')).toBeInTheDocument()
  })
})

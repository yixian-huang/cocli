import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { ActivityPanel } from './ActivityPanel'
import { useAgentStore } from '@/stores/agentStore'
import { resetPrefsStore, applyPrefsFromServer } from '@/stores/prefsStore'

const { agentTurnsList, agentSessionsList } = vi.hoisted(() => ({
  agentTurnsList: vi.fn(),
  agentSessionsList: vi.fn(),
}))

vi.mock('@/api/client', () => ({
  agents: {
    list: vi.fn(),
    start: vi.fn(),
    stop: vi.fn(),
    cancelTurn: vi.fn(),
    steerTurn: vi.fn(),
  },
  agentTurns: { list: agentTurnsList },
  agentSessions: { list: agentSessionsList },
}))

describe('ActivityPanel loading skeleton', () => {
  beforeEach(() => {
    agentTurnsList.mockResolvedValue([])
    agentSessionsList.mockResolvedValue([])
    useAgentStore.setState({
      agents: [],
      loading: false,
      turns: {},
      currentTurnEntries: {},
    })
  })

  afterEach(() => {
    cleanup()
    vi.restoreAllMocks()
  })

  it('shows skeletons while loading and removes them once loading is false', () => {
    const { rerender } = render(<ActivityPanel agentId="agent-1" loading />)

    expect(screen.getByTestId('turn-log-session-skeleton')).toBeInTheDocument()
    expect(screen.getByTestId('turn-log-skeleton')).toBeInTheDocument()

    rerender(<ActivityPanel agentId="agent-1" loading={false} />)

    expect(screen.queryByTestId('turn-log-session-skeleton')).not.toBeInTheDocument()
    expect(screen.queryByTestId('turn-log-skeleton')).not.toBeInTheDocument()
    expect(screen.getByText('No turns recorded yet')).toBeInTheDocument()
  })
})

describe('ActivityPanel bulk toolbar', () => {
  beforeEach(() => {
    agentTurnsList.mockResolvedValue([])
    agentSessionsList.mockResolvedValue([])
    resetPrefsStore()
    useAgentStore.setState({
      agents: [],
      loading: false,
      turns: {},
      currentTurnEntries: {},
    })
  })

  afterEach(() => {
    cleanup()
    vi.restoreAllMocks()
  })

  it('renders Expand all / Collapse all / Latest buttons in timeline view', () => {
    render(<ActivityPanel agentId="agent-1" loading={false} />)
    expect(screen.getByRole('button', { name: /expand all/i })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /collapse all/i })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /latest/i })).toBeInTheDocument()
  })

  it('reads defaultExpandLastN from prefsStore (no crash with custom value)', () => {
    applyPrefsFromServer({ ui: { activity: { defaultExpandLastN: 1 } } })
    render(<ActivityPanel agentId="agent-1" loading={false} />)
    expect(screen.getByRole('button', { name: /expand all/i })).toBeInTheDocument()
  })
})

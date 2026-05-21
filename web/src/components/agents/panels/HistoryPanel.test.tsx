import type { ReactNode } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { HistoryPanel } from './HistoryPanel'
import { useAgentStore } from '@/stores/agentStore'

const { agentSessionsList, agentTurnsList } = vi.hoisted(() => ({
  agentSessionsList: vi.fn(),
  agentTurnsList: vi.fn(),
}))

vi.mock('@/api/client', () => ({
  agents: { list: vi.fn(), start: vi.fn(), stop: vi.fn(), cancelTurn: vi.fn(), steerTurn: vi.fn() },
  agentSessions: { list: agentSessionsList },
  agentTurns: { list: agentTurnsList, listBySession: vi.fn() },
}))

beforeEach(() => {
  agentSessionsList.mockResolvedValue([])
  agentTurnsList.mockResolvedValue([])
  useAgentStore.setState({ agents: [], loading: false, turns: {}, currentTurnEntries: {} })
})

afterEach(() => { cleanup(); vi.restoreAllMocks() })

function renderInRouter(ui: ReactNode) {
  return render(<MemoryRouter>{ui}</MemoryRouter>)
}

describe('HistoryPanel', () => {
  it('defaults to Sessions segment', () => {
    renderInRouter(<HistoryPanel agentId="a" />)
    expect(screen.getByRole('button', { name: /^sessions$/i })).toHaveAttribute('data-active', 'true')
  })

  it('switches to Activity segment on click', () => {
    renderInRouter(<HistoryPanel agentId="a" />)
    fireEvent.click(screen.getByRole('button', { name: /^activity$/i }))
    expect(screen.getByRole('button', { name: /^activity$/i })).toHaveAttribute('data-active', 'true')
    expect(screen.getByRole('button', { name: /^sessions$/i })).toHaveAttribute('data-active', 'false')
  })

  it('respects initialSegment prop', () => {
    renderInRouter(<HistoryPanel agentId="a" initialSegment="activity" />)
    expect(screen.getByRole('button', { name: /^activity$/i })).toHaveAttribute('data-active', 'true')
  })
})

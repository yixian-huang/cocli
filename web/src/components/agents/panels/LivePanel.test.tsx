import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { LivePanel } from './LivePanel'
import { useAgentStore } from '@/stores/agentStore'
import { useViewStore } from '@/stores/viewStore'

beforeEach(() => {
  useAgentStore.setState({
    agents: [{
      id: 'a',
      name: 'codex',
      status: 'working',
      contextWindow: 10000,
      lastInputTokens: 6200,
      totalOutputTokens: 1200,
      totalCostUSD: 0.42,
      turnCount: 12,
    } as never],
    loading: false,
    turns: {},
    currentTurnEntries: {
      a: [
        { id: 'e1', kind: 'text', text: 'hello world', ts: 1747216800000 },
        { id: 'e2', kind: 'tool_call', text: 'read', ts: 1747216805000 },
      ] as never,
    },
  })
  useViewStore.setState({
    activeAgentId: 'a',
    quotedMessage: null,
    agentSubview: {},
    activeDrawer: 'live',
    historyDrawerSegment: null,
  })
})

afterEach(() => cleanup())

describe('LivePanel', () => {
  it('renders ContextBar full variant with agent values', () => {
    render(<LivePanel agentId="a" />)
    expect(screen.getByText(/62% context/)).toBeInTheDocument()
    expect(screen.getByText(/12 turns/)).toBeInTheDocument()
  })

  it('renders recent activity rows from currentTurnEntries', () => {
    render(<LivePanel agentId="a" />)
    expect(screen.getByText(/hello world/)).toBeInTheDocument()
    expect(screen.getByText(/tool_call/)).toBeInTheDocument()
  })

  it('view full activity button opens history drawer at activity segment', () => {
    render(<LivePanel agentId="a" />)
    fireEvent.click(screen.getByRole('button', { name: /view full activity/i }))
    expect(useViewStore.getState().activeDrawer).toBe('history')
    expect(useViewStore.getState().historyDrawerSegment).toBe('activity')
  })

  it('shows offline placeholder when no agent context data', () => {
    useAgentStore.setState({
      agents: [{ id: 'a', name: 'codex', status: 'offline' } as never],
      currentTurnEntries: {},
    })
    render(<LivePanel agentId="a" />)
    expect(screen.getByText(/no live data/i)).toBeInTheDocument()
  })
})

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { AgentPanel } from './AgentPanel'
import { AgentView } from '@/components/agents/AgentView'
import { useAgentStore } from '@/stores/agentStore'
import { useUserStore } from '@/stores/userStore'
import { useViewStore } from '@/stores/viewStore'
import type { Agent, User } from '@/lib/types'

const now = new Date().toISOString()
const fetchMock = vi.fn()

const makeAgent = (runtime: string): Agent => ({
  id: `agent-${runtime}`,
  name: `${runtime}-agent`,
  runtime,
  model: 'gpt-5.4',
  status: 'working',
  zoneId: 'zone-test',
  createdAt: now,
  updatedAt: now,
})

const setCurrentUser = (role: User['role']) => {
  useUserStore.setState({
    user: {
      id: `${role}-id`,
      name: `${role}-user`,
      role,
      hasPassword: true,
      createdAt: now,
    },
  })
}

describe('AgentPanel runtime turn control gating', () => {
  beforeEach(() => {
    setCurrentUser('admin')
    fetchMock.mockReset()
    vi.stubGlobal('fetch', fetchMock)
  })

  afterEach(() => {
    useAgentStore.setState({ agents: [], loading: false })
    useViewStore.setState({ activeAgentId: null, quotedMessage: null })
    vi.unstubAllGlobals()
    cleanup()
  })

  it('enables cancel/steer controls for codex runtime without unsupported tooltip', () => {
    render(<AgentPanel agent={makeAgent('codex')} />)

    const steerInput = screen.getByPlaceholderText('Steer...')
    fireEvent.change(steerInput, { target: { value: 'keep going' } })

    const sendButton = screen.getByRole('button', { name: 'Send' })
    const cancelTurnButton = screen.getByRole('button', { name: 'Cancel turn' })

    expect(sendButton).toBeEnabled()
    expect(cancelTurnButton).toBeEnabled()
    expect(sendButton.closest('span')).not.toHaveAttribute('title')
    expect(cancelTurnButton.closest('span')).not.toHaveAttribute('title')
  })

  it('disables cancel/steer controls for claude runtime with unsupported tooltip', () => {
    render(<AgentPanel agent={makeAgent('claude')} />)

    const sendButton = screen.getByRole('button', { name: 'Send' })
    const cancelTurnButton = screen.getByRole('button', { name: 'Cancel turn' })

    expect(sendButton).toBeDisabled()
    expect(cancelTurnButton).toBeDisabled()
    expect(sendButton.closest('span')).toHaveAttribute('title', expect.stringContaining('Unsupported'))
    expect(cancelTurnButton.closest('span')).toHaveAttribute('title', expect.stringContaining('Unsupported'))
  })

  it('disables cancel/steer controls for gemini runtime with unsupported tooltip', () => {
    render(<AgentPanel agent={makeAgent('gemini')} />)

    const sendButton = screen.getByRole('button', { name: 'Send' })
    const cancelTurnButton = screen.getByRole('button', { name: 'Cancel turn' })

    expect(sendButton).toBeDisabled()
    expect(cancelTurnButton).toBeDisabled()
    expect(sendButton.closest('span')).toHaveAttribute('title', expect.stringContaining('Unsupported'))
    expect(cancelTurnButton.closest('span')).toHaveAttribute('title', expect.stringContaining('Unsupported'))
  })

  it('hides turn controls for non-admin users', () => {
    setCurrentUser('member')
    render(<AgentPanel agent={makeAgent('codex')} />)

    expect(screen.queryByRole('button', { name: 'Send' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Cancel turn' })).not.toBeInTheDocument()
    expect(screen.queryByPlaceholderText('Steer...')).not.toBeInTheDocument()
  })

  it('renders semantic badges for overflow and rate-limited attention states', () => {
    render(
      <>
        <AgentPanel
          agent={{
            ...makeAgent('codex'),
            id: 'agent-overflow',
            name: 'overflow-agent',
            attentionState: 'context_overflow',
          }}
        />
        <AgentPanel
          agent={{
            ...makeAgent('codex'),
            id: 'agent-rate-limited',
            name: 'rate-limited-agent',
            attentionState: 'rate_limited',
          }}
        />
      </>
    )

    const overflowBadge = screen.getByText('Context overflow').parentElement
    const rateLimitedBadge = screen.getByText('Rate limited').parentElement

    expect(overflowBadge).toHaveClass('rounded-full', 'text-error')
    expect(overflowBadge).toHaveAttribute('title', 'Context window overflow was detected')
    expect(rateLimitedBadge).toHaveClass('rounded-full', 'text-warning')
    expect(rateLimitedBadge).toHaveAttribute('title', 'Provider rate limits are slowing responses')
  })
})

describe('AgentView overflow tab (via Settings)', () => {
  beforeEach(() => {
    setCurrentUser('admin')
    fetchMock.mockReset()
    vi.stubGlobal('fetch', fetchMock)
  })

  afterEach(() => {
    useAgentStore.setState({ agents: [], loading: false })
    useViewStore.setState({
      activeAgentId: null,
      quotedMessage: null,
      agentSubview: {},
      activeDrawer: null,
    })
    vi.unstubAllGlobals()
    cleanup()
  })

  it('renders overflow telemetry for admins inside Settings', async () => {
    const agent = {
      ...makeAgent('claude'),
      id: 'agent-overflow',
      model: 'sonnet-4-6',
    }
    useAgentStore.setState({ agents: [agent], loading: false })
    useViewStore.setState({
      activeAgentId: agent.id,
      quotedMessage: null,
      agentSubview: {},
      activeDrawer: null,
    })

    fetchMock.mockResolvedValue({
      ok: true,
      json: async () => ([
        {
          driver: 'claude',
          model: 'sonnet-4-6',
          currentBackstopPct: 0.91,
          overflowCount: 2,
          recentOverflows: [
            {
              utilPct: 0.97,
              occurredAt: now,
              sessionAgeSeconds: 180,
              contextWindowTokens: 200000,
            },
          ],
          forksSinceLastOverflow: 3,
          lastAdjustedAt: now,
          contextWindowTokens: 200000,
          defaultBackstopPct: 0.95,
        },
      ]),
      text: async () => '[]',
    })

    render(<AgentView />)
    fireEvent.click(screen.getByRole('button', { name: /open settings/i }))
    fireEvent.click(screen.getByRole('button', { name: /^overflow$/i }))

    expect(await screen.findByText('Current Backstop')).toBeInTheDocument()
    expect(screen.getByText('91%')).toBeInTheDocument()
    expect(screen.getByText('Overflow Count')).toBeInTheDocument()
    expect(screen.getByText('Successful forks since last overflow')).toBeInTheDocument()
    expect(screen.getByText('Overflow at 97%')).toBeInTheDocument()
    expect(screen.getByText(/Backstop thresholds adapt from real overflow signals/)).toBeInTheDocument()
    expect(fetchMock).toHaveBeenCalled()
  })

  it('hides the Overflow sub-tab for non-admin users in Settings', () => {
    setCurrentUser('member')
    const agent = {
      ...makeAgent('codex'),
      id: 'agent-overflow-member',
      model: 'gpt-5-codex',
    }
    useAgentStore.setState({ agents: [agent], loading: false })
    useViewStore.setState({
      activeAgentId: agent.id,
      quotedMessage: null,
      agentSubview: {},
      activeDrawer: null,
    })

    render(<AgentView />)
    fireEvent.click(screen.getByRole('button', { name: /open settings/i }))

    expect(screen.queryByRole('button', { name: /^overflow$/i })).not.toBeInTheDocument()
  })

  it('Esc in Settings returns to main mode', () => {
    setCurrentUser('member')
    const agent = { ...makeAgent('codex'), id: 'agent-esc' }
    useAgentStore.setState({ agents: [agent], loading: false })
    useViewStore.setState({
      activeAgentId: agent.id,
      quotedMessage: null,
      agentSubview: {},
      activeDrawer: null,
    })

    render(<AgentView />)
    fireEvent.click(screen.getByRole('button', { name: /open settings/i }))
    expect(useViewStore.getState().getSubview(agent.id)).toBe('settings')

    fireEvent.keyDown(window, { key: 'Escape' })
    expect(useViewStore.getState().getSubview(agent.id)).toBe('main')
  })
})

describe('AgentView new header', () => {
  beforeEach(() => {
    setCurrentUser('member')
    fetchMock.mockReset()
    vi.stubGlobal('fetch', fetchMock)
  })

  afterEach(() => {
    useAgentStore.setState({ agents: [], loading: false })
    useViewStore.setState({
      activeAgentId: null,
      quotedMessage: null,
      agentSubview: {},
      activeDrawer: null,
    })
    vi.unstubAllGlobals()
    cleanup()
  })

  it('renders Live / History / Memory / Settings icons + inline ContextBar', () => {
    const agent = {
      ...makeAgent('codex'),
      id: 'agent-header',
      contextWindow: 10000,
      lastInputTokens: 6200,
      totalOutputTokens: 1200,
      totalCostUSD: 0.42,
      turnCount: 12,
    }
    useAgentStore.setState({ agents: [agent], loading: false })
    useViewStore.setState({
      activeAgentId: agent.id,
      quotedMessage: null,
      agentSubview: {},
      activeDrawer: null,
    })
    render(<AgentView />)

    expect(screen.getByRole('button', { name: /open live/i })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /open history/i })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /open memory/i })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /open settings/i })).toBeInTheDocument()
    expect(screen.getByTestId('context-bar-inline')).toBeInTheDocument()
  })

  it('Live icon click toggles activeDrawer', () => {
    const agent = { ...makeAgent('codex'), id: 'agent-header' }
    useAgentStore.setState({ agents: [agent], loading: false })
    useViewStore.setState({
      activeAgentId: agent.id,
      quotedMessage: null,
      agentSubview: {},
      activeDrawer: null,
    })
    render(<AgentView />)

    fireEvent.click(screen.getByRole('button', { name: /open live/i }))
    expect(useViewStore.getState().activeDrawer).toBe('live')
    fireEvent.click(screen.getByRole('button', { name: /open live/i }))
    expect(useViewStore.getState().activeDrawer).toBeNull()
  })
})

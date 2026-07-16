import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { AgentList } from './AgentList'
import { useAgentStore } from '@/stores/agentStore'
import { useUserStore } from '@/stores/userStore'
import { ContextMenuPortal } from '@/components/ui/ContextMenu'
import type { Agent, User } from '@/lib/types'

vi.mock('react-router-dom', async () => {
  const actual = await vi.importActual<typeof import('react-router-dom')>('react-router-dom')
  return {
    ...actual,
    useNavigate: () => () => {},
  }
})

function makeAgent(over: Partial<Agent> & Pick<Agent, 'id' | 'name'>): Agent {
  return {
    status: 'offline',
    runtime: 'claude',
    ...over,
  } as Agent
}

describe('<AgentList>', () => {
  beforeEach(() => {
    useUserStore.setState({ user: { id: 'u1', name: 'me', role: 'member' } as User })
    useAgentStore.setState({
      agents: [
        makeAgent({ id: 'a1', name: 'codex-backend' }),
        makeAgent({ id: 'a2', name: 'claude-ops' }),
      ],
    })
  })

  afterEach(() => {
    fireEvent.keyDown(document.body, { key: 'Escape' })
    cleanup()
    vi.restoreAllMocks()
  })

  it('renders inside a CollapsibleSection and shows agents', () => {
    render(
      <>
        <ContextMenuPortal />
        <AgentList />
      </>,
    )
    expect(screen.getByRole('button', { name: /agents/i })).toBeInTheDocument()
    expect(screen.getByText(/codex-backend/)).toBeInTheDocument()
    expect(screen.getByText(/claude-ops/)).toBeInTheDocument()
  })

  it('admin gets Rename + Delete in row context menu', () => {
    useUserStore.setState({ user: { id: 'u1', name: 'me', role: 'admin' } as User })
    render(
      <>
        <ContextMenuPortal />
        <AgentList />
      </>,
    )
    fireEvent.contextMenu(screen.getByText(/codex-backend/), { clientX: 10, clientY: 10 })
    expect(screen.getByText('Rename')).toBeInTheDocument()
    expect(screen.getByText(/delete/i)).toBeInTheDocument()
  })

  it('non-admin gets no menu items (right-click is a no-op)', () => {
    render(
      <>
        <ContextMenuPortal />
        <AgentList />
      </>,
    )
    fireEvent.contextMenu(screen.getByText(/codex-backend/), { clientX: 10, clientY: 10 })
    expect(screen.queryByText('Rename')).toBeNull()
  })
})

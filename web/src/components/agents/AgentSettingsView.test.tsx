import type { ReactNode } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { AgentSettingsView } from './AgentSettingsView'
import { useAgentStore } from '@/stores/agentStore'

vi.mock('./ProfileTab', () => ({ ProfileTab: () => <div data-testid="settings-profile" /> }))
vi.mock('./MemoryTab', () => ({ MemoryTab: () => <div data-testid="settings-memory" /> }))
vi.mock('./OverflowTab', () => ({ OverflowTab: () => <div data-testid="settings-overflow" /> }))

beforeEach(() => {
  useAgentStore.setState({
    agents: [{ id: 'a', name: 'codex', status: 'offline' } as never],
    loading: false,
    turns: {},
    currentTurnEntries: {},
  })
})

afterEach(() => cleanup())

function renderInRouter(ui: ReactNode) {
  return render(<MemoryRouter>{ui}</MemoryRouter>)
}

describe('AgentSettingsView', () => {
  it('shows Profile, Memory, and Overflow tabs (single-tenant always admin)', () => {
    renderInRouter(<AgentSettingsView agentId="a" />)
    expect(screen.getByRole('tab', { name: /^profile$/i })).toBeInTheDocument()
    expect(screen.getByRole('tab', { name: /^memory$/i })).toBeInTheDocument()
    expect(screen.getByRole('tab', { name: /^overflow$/i })).toBeInTheDocument()
    expect(screen.queryByRole('tab', { name: /^skills$/i })).not.toBeInTheDocument()
  })

  it('defaults to Profile sub-tab', () => {
    renderInRouter(<AgentSettingsView agentId="a" />)
    expect(screen.getByTestId('settings-profile')).toBeInTheDocument()
    expect(screen.queryByTestId('settings-memory')).not.toBeInTheDocument()
  })

  it('clicking Memory switches sub-tab', () => {
    renderInRouter(<AgentSettingsView agentId="a" />)
    fireEvent.click(screen.getByRole('tab', { name: /^memory$/i }))
    expect(screen.getByTestId('settings-memory')).toBeInTheDocument()
    expect(screen.queryByTestId('settings-profile')).not.toBeInTheDocument()
  })
})

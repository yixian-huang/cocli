import type { ReactNode } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { AgentSettingsView } from './AgentSettingsView'
import { useUserStore } from '@/stores/userStore'
import { useAgentStore } from '@/stores/agentStore'
import { useFeatureFlagStore } from '@/stores/featureFlagStore'

vi.mock('./ProfileTab', () => ({ ProfileTab: () => <div data-testid="settings-profile" /> }))
vi.mock('./SkillsTab', () => ({ SkillsTab: () => <div data-testid="settings-skills" /> }))
vi.mock('./MemoryTab', () => ({ MemoryTab: () => <div data-testid="settings-memory" /> }))
vi.mock('./OverflowTab', () => ({ OverflowTab: () => <div data-testid="settings-overflow" /> }))

beforeEach(() => {
  useAgentStore.setState({
    agents: [{ id: 'a', name: 'codex', status: 'offline' } as never],
    loading: false,
    turns: {},
    currentTurnEntries: {},
  })
  // Default: skills_v2 on for existing tests
  useFeatureFlagStore.setState({ flags: { skills_v2: true } })
})

afterEach(() => cleanup())

function renderInRouter(ui: ReactNode) {
  return render(<MemoryRouter>{ui}</MemoryRouter>)
}

describe('AgentSettingsView', () => {
  it('non-admin sees Profile, Skills, Memory — but not Overflow', () => {
    useUserStore.setState({ user: { id: 'u', name: 'alice', role: 'member' } as never })
    renderInRouter(<AgentSettingsView agentId="a" />)
    expect(screen.getByRole('tab', { name: /^profile$/i })).toBeInTheDocument()
    expect(screen.getByRole('tab', { name: /^skills$/i })).toBeInTheDocument()
    expect(screen.getByRole('tab', { name: /^memory$/i })).toBeInTheDocument()
    expect(screen.queryByRole('tab', { name: /^overflow$/i })).not.toBeInTheDocument()
  })

  it('admin also sees Overflow', () => {
    useUserStore.setState({ user: { id: 'u', name: 'admin', role: 'admin' } as never })
    renderInRouter(<AgentSettingsView agentId="a" />)
    expect(screen.getByRole('tab', { name: /^overflow$/i })).toBeInTheDocument()
  })

  it('defaults to Profile sub-tab', () => {
    useUserStore.setState({ user: { id: 'u', name: 'alice', role: 'member' } as never })
    renderInRouter(<AgentSettingsView agentId="a" />)
    expect(screen.getByTestId('settings-profile')).toBeInTheDocument()
    expect(screen.queryByTestId('settings-skills')).not.toBeInTheDocument()
  })

  it('clicking Skills switches sub-tab', () => {
    useUserStore.setState({ user: { id: 'u', name: 'alice', role: 'member' } as never })
    renderInRouter(<AgentSettingsView agentId="a" />)
    fireEvent.click(screen.getByRole('tab', { name: /^skills$/i }))
    expect(screen.getByTestId('settings-skills')).toBeInTheDocument()
    expect(screen.queryByTestId('settings-profile')).not.toBeInTheDocument()
  })

  it('hides Skills tab when skills_v2 feature flag is off', () => {
    useUserStore.setState({ user: { id: 'u', name: 'alice', role: 'member' } as never })
    useFeatureFlagStore.setState({ flags: { skills_v2: false } })
    renderInRouter(<AgentSettingsView agentId="a" />)
    expect(screen.queryByRole('button', { name: /^skills$/i })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: /^profile$/i })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /^memory$/i })).toBeInTheDocument()
  })
})

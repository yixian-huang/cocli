import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { SkillsTab } from './SkillsTab'
import { useAgentSkillStore } from '@/stores/agentSkillStore'
import { useAgentStore } from '@/stores/agentStore'

const noopFetch = vi.fn().mockResolvedValue(undefined)
const noopCompat = vi.fn().mockResolvedValue(undefined)

beforeEach(() => {
  useAgentSkillStore.setState({
    skillsByAgent: {
      'a1': [
        { name: 'wikic', userInvocable: true, type: 'workspace', state: 'managed', installId: 'i1', libraryId: 'lib1' },
        { name: 'hand-placed', userInvocable: false, type: 'workspace', state: 'external' },
        { name: 'gone', userInvocable: false, type: 'workspace', state: 'broken', installId: 'i2', libraryId: 'lib2' },
      ],
    },
    loadingByAgent: { 'a1': false },
    compatibility: { claude: 'supported', chatrs: 'unsupported' },
    errorByAgent: {},
    fetchForAgent: noopFetch,
    loadCompatibility: noopCompat,
  } as any)
  useAgentStore.setState({
    agents: [{ id: 'a1', runtime: 'claude', status: 'online', zoneId: 'z1' } as any],
  } as any)
})

describe('SkillsTab', () => {
  it('renders all three states with badges', () => {
    render(<SkillsTab agentId="a1" offline={false} />)
    expect(screen.getByText('wikic')).toBeInTheDocument()
    expect(screen.getByText('hand-placed')).toBeInTheDocument()
    expect(screen.getByText('gone')).toBeInTheDocument()
    expect(screen.getByText('managed')).toBeInTheDocument()
    expect(screen.getByText('external')).toBeInTheDocument()
    expect(screen.getByText('broken')).toBeInTheDocument()
  })

  it('search filters skills by name', async () => {
    render(<SkillsTab agentId="a1" offline={false} />)
    fireEvent.change(screen.getByPlaceholderText(/search/i), { target: { value: 'wikic' } })
    await waitFor(() => {
      expect(screen.queryByText('hand-placed')).not.toBeInTheDocument()
    })
  })

  it('install button disabled for chatrs runtime', () => {
    useAgentStore.setState({
      agents: [{ id: 'a1', runtime: 'chatrs', status: 'online', zoneId: 'z1' } as any],
    } as any)
    // keep noop fetchers so component doesn't re-trigger loading state
    useAgentSkillStore.setState({ fetchForAgent: noopFetch, loadCompatibility: noopCompat } as any)
    render(<SkillsTab agentId="a1" offline={false} />)
    const btn = screen.getByRole('button', { name: /install from library/i })
    expect(btn).toBeDisabled()
  })

  it('uninstall on managed row calls store.uninstall', async () => {
    const uninstallSpy = vi.fn().mockResolvedValue(undefined)
    useAgentSkillStore.setState({ uninstall: uninstallSpy } as any)
    render(<SkillsTab agentId="a1" offline={false} />)
    fireEvent.click(screen.getAllByRole('button', { name: /uninstall/i })[0])
    await waitFor(() => {
      expect(uninstallSpy).toHaveBeenCalledWith('a1', 'i1')
    })
  })
})

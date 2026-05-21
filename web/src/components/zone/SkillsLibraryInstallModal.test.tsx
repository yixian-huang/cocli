import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { SkillsLibraryInstallModal } from './SkillsLibraryInstallModal'
import * as client from '@/api/client'
import { useAgentSkillStore } from '@/stores/agentSkillStore'

beforeEach(() => {
  vi.restoreAllMocks()
  useAgentSkillStore.setState({
    compatibility: { claude: 'supported', codex: 'supported', chatrs: 'unsupported', gemini: 'uncertain' },
    loadCompatibility: vi.fn().mockResolvedValue(undefined),
  } as any)
  vi.spyOn(client.agents, 'list').mockResolvedValue([
    { id: 'a1', name: 'ops', runtime: 'claude', zoneId: 'z1' } as any,
    { id: 'a2', name: 'cdr', runtime: 'codex', zoneId: 'z1' } as any,
    { id: 'a3', name: 'rs', runtime: 'chatrs', zoneId: 'z1' } as any,
  ])
  vi.spyOn(client.zoneSkillLibrary, 'list').mockResolvedValue({
    entries: [{ id: 'lib1', name: 'wikic', displayName: 'wikic' } as any],
  })
})

describe('SkillsLibraryInstallModal', () => {
  it('disables chatrs row', async () => {
    render(<SkillsLibraryInstallModal zoneId="z1" onClose={() => {}} onInstalled={() => {}} />)
    await waitFor(() => screen.getByText('rs'))
    const chatrsCheckbox = screen.getByLabelText(/rs/i) as HTMLInputElement
    expect(chatrsCheckbox.disabled).toBe(true)
  })

  it('Promise.allSettled fires install for each selected agent', async () => {
    const installSpy = vi.spyOn(client.agentSkills, 'install').mockResolvedValue({
      installId: 'i', installPath: '.claude/skills/wikic',
    })
    render(<SkillsLibraryInstallModal zoneId="z1" onClose={() => {}} onInstalled={() => {}} />)
    await waitFor(() => screen.getByText('ops'))
    // Select library entry first
    fireEvent.click(screen.getByText('wikic'))
    fireEvent.click(screen.getByLabelText(/ops/i))
    fireEvent.click(screen.getByLabelText(/cdr/i))
    fireEvent.click(screen.getByRole('button', { name: /^install$/i }))
    await waitFor(() => {
      expect(installSpy).toHaveBeenCalledWith('a1', 'lib1')
      expect(installSpy).toHaveBeenCalledWith('a2', 'lib1')
    })
  })

  it('renders per-row failure feedback on partial success', async () => {
    vi.spyOn(client.agentSkills, 'install').mockImplementation(async (agentId) => {
      if (agentId === 'a2') throw new Error('boom')
      return { installId: 'i', installPath: '' }
    })
    render(<SkillsLibraryInstallModal zoneId="z1" onClose={() => {}} onInstalled={() => {}} />)
    await waitFor(() => screen.getByText('ops'))
    fireEvent.click(screen.getByText('wikic'))
    fireEvent.click(screen.getByLabelText(/ops/i))
    fireEvent.click(screen.getByLabelText(/cdr/i))
    fireEvent.click(screen.getByRole('button', { name: /^install$/i }))
    await waitFor(() => {
      expect(screen.getByText(/boom/i)).toBeInTheDocument()
    })
  })
})

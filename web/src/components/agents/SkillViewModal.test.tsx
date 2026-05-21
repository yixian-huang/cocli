import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import { SkillViewModal } from './SkillViewModal'
import * as client from '@/api/client'
import type { SkillView } from '@/lib/types'

const skill: SkillView = {
  name: 'wikic', userInvocable: true, type: 'workspace', state: 'managed',
  installId: 'i1', libraryId: 'lib1',
}

beforeEach(() => {
  vi.restoreAllMocks()
  vi.spyOn(client.agentSkills, 'listFiles').mockResolvedValue({
    installPath: '.claude/skills/wikic',
    files: [
      { name: 'SKILL.md', isDir: false, size: 200 },
      { name: 'scripts', isDir: true },
    ],
  })
  vi.spyOn(client.agentSkills, 'getFile').mockResolvedValue({
    content: '# wikic\n\nBody.', binary: false,
  })
})

describe('SkillViewModal', () => {
  it('loads SKILL.md by default and renders markdown', async () => {
    render(<SkillViewModal agentId="a1" skill={skill} onClose={() => {}} />)
    await waitFor(() => {
      expect(screen.getByText('wikic', { selector: 'h1' })).toBeInTheDocument()
    })
  })

  it('shows binary placeholder when getFile returns binary=true', async () => {
    vi.spyOn(client.agentSkills, 'getFile').mockResolvedValue({
      content: '', binary: true,
    })
    vi.spyOn(client.agentSkills, 'listFiles').mockResolvedValue({
      installPath: '.claude/skills/wikic',
      files: [{ name: 'icon.png', isDir: false, size: 4096 }],
    })
    render(<SkillViewModal agentId="a1" skill={skill} onClose={() => {}} />)
    await waitFor(() => {
      expect(screen.getByText(/binary/i)).toBeInTheDocument()
    })
  })
})

import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import { SkillsLibraryDetailModal } from './SkillsLibraryDetailModal'
import type { SkillLibraryEntry } from '@/lib/types'

vi.mock('@/api/client', () => ({
  zoneSkillLibrary: {
    get: vi.fn().mockResolvedValue({
      entry: {
        id: 'lib-1', zoneId: 'z1', name: 'sample', sourceUrl: 'https://x/y',
        userInvocable: true, sourceKind: 'git', totalBytes: 1024, fileCount: 3,
        importedBy: 'u1', importedAt: '2026-05-20T00:00:00Z', updatedAt: '2026-05-20T00:00:00Z',
      },
      files: [{ relPath: 'SKILL.md', size: 100, mode: 0o644 }],
    }),
    getFile: vi.fn().mockResolvedValue({ content: '# Sample\nDescription text.', binary: false, size: 100 }),
  },
}))

const baseEntry: SkillLibraryEntry = {
  id: 'lib-1', zoneId: 'z1', name: 'sample',
  sourceUrl: 'https://x/y', sourceKind: 'git', userInvocable: true,
  totalBytes: 1024, fileCount: 3, importedBy: 'u1',
  importedAt: '2026-05-20T00:00:00Z', updatedAt: '2026-05-20T00:00:00Z',
}

afterEach(() => cleanup())

describe('SkillsLibraryDetailModal', () => {
  it('shows skill name + source', async () => {
    render(<SkillsLibraryDetailModal open zoneId="z1" entry={baseEntry} onClose={() => {}} />)
    expect(await screen.findByText('sample')).toBeInTheDocument()
    expect(screen.getByText(/https:\/\/x\/y/)).toBeInTheDocument()
  })

  it('renders SKILL.md preview after loading', async () => {
    render(<SkillsLibraryDetailModal open zoneId="z1" entry={baseEntry} onClose={() => {}} />)
    await waitFor(() => expect(screen.getByText(/description text/i)).toBeInTheDocument())
  })

  it('shows empty-state for in-use agents (phase 2 stub returns [])', async () => {
    render(<SkillsLibraryDetailModal open zoneId="z1" entry={baseEntry} onClose={() => {}} />)
    expect(await screen.findByText(/no agents are currently using this skill/i)).toBeInTheDocument()
  })

  it('returns null when entry is null', () => {
    const { container } = render(
      <SkillsLibraryDetailModal open={false} zoneId="z1" entry={null} onClose={() => {}} />
    )
    expect(container.firstChild).toBeNull()
  })
})

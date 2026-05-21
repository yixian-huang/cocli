import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { SkillsLibraryTab } from './SkillsLibraryTab'
import type { SkillLibraryEntry } from '@/lib/types'

const listMock = vi.fn()
const reinstallMock = vi.fn()
const removeMock = vi.fn()

vi.mock('@/api/client', () => ({
  zoneSkillLibrary: {
    list: (...a: unknown[]) => listMock(...a),
    reinstall: (...a: unknown[]) => reinstallMock(...a),
    remove: (...a: unknown[]) => removeMock(...a),
    get: vi.fn().mockResolvedValue({ entry: {}, files: [] }),
    getFile: vi.fn().mockResolvedValue({ content: '', binary: false, size: 0 }),
  },
  ApiError: class ApiError extends Error { status = 0; requestId = '' },
}))

const sample = (over: Partial<SkillLibraryEntry> = {}): SkillLibraryEntry => ({
  id: 'lib-1', zoneId: 'z1', name: 'pdf-tools',
  displayName: 'PDF Tools', description: 'work with pdfs',
  sourceUrl: 'https://github.com/x/pdf', sourceKind: 'git',
  sourceRef: '1234567890abcdef1234567890abcdef12345678',
  userInvocable: true, totalBytes: 184_320, fileCount: 12,
  importedBy: 'u1', importedAt: '2026-05-20T00:00:00Z',
  updatedAt: '2026-05-20T00:00:00Z', inUseCount: 3,
  ...over,
})

beforeEach(() => {
  listMock.mockReset()
  reinstallMock.mockReset()
  removeMock.mockReset()
})
afterEach(() => cleanup())

describe('SkillsLibraryTab table', () => {
  it('renders one row per entry with name, short ref, size, and in-use count', async () => {
    listMock.mockResolvedValue({ entries: [sample(), sample({ id: 'lib-2', name: 'csv-tools', displayName: 'CSV Tools', inUseCount: 0 })] })
    render(<SkillsLibraryTab zoneId="z1" />)
    expect(await screen.findByText('PDF Tools')).toBeInTheDocument()
    expect(screen.getByText('CSV Tools')).toBeInTheDocument()
    // Both entries share the same SHA prefix — use getAllByText
    expect(screen.getAllByText('1234567').length).toBeGreaterThan(0)
    // Both rows also share size/file-count — assert at least one matches
    expect(screen.getAllByText(/180\.0 KB · 12 files/).length).toBeGreaterThan(0)
  })

  it('rows with inUseCount=0 are visually dimmed', async () => {
    listMock.mockResolvedValue({ entries: [sample({ inUseCount: 0 })] })
    render(<SkillsLibraryTab zoneId="z1" />)
    const row = (await screen.findByText('PDF Tools')).closest('tr')!
    expect(row.className).toContain('opacity-65')
  })

  it('search filters by name / description / URL', async () => {
    listMock.mockResolvedValue({
      entries: [
        sample({ id: 'a', name: 'pdf-tools', displayName: 'PDF Tools' }),
        sample({ id: 'b', name: 'csv-tools', displayName: 'CSV Tools', sourceUrl: 'https://github.com/x/csv' }),
      ],
    })
    render(<SkillsLibraryTab zoneId="z1" />)
    await screen.findByText('PDF Tools')
    fireEvent.change(screen.getByPlaceholderText(/search by name/i), { target: { value: 'csv' } })
    expect(screen.queryByText('PDF Tools')).not.toBeInTheDocument()
    expect(screen.getByText('CSV Tools')).toBeInTheDocument()
  })

  it('reinstall action calls reinstall then refreshes', async () => {
    listMock.mockResolvedValue({ entries: [sample()] })
    reinstallMock.mockResolvedValue({ updated: true })
    render(<SkillsLibraryTab zoneId="z1" />)
    const row = (await screen.findByText('PDF Tools')).closest('tr')!
    fireEvent.click(within(row).getByLabelText(/reinstall pdf-tools/i))
    await waitFor(() => expect(reinstallMock).toHaveBeenCalledWith('z1', 'lib-1'))
    // refresh ⇒ list called twice (initial + after reinstall)
    await waitFor(() => expect(listMock).toHaveBeenCalledTimes(2))
  })

  it('delete action shows confirm and only deletes on confirmation', async () => {
    listMock.mockResolvedValue({ entries: [sample()] })
    removeMock.mockResolvedValue({ deleted: 'lib-1' })
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true)
    render(<SkillsLibraryTab zoneId="z1" />)
    const row = (await screen.findByText('PDF Tools')).closest('tr')!
    fireEvent.click(within(row).getByLabelText(/delete pdf-tools/i))
    expect(confirmSpy).toHaveBeenCalled()
    await waitFor(() => expect(removeMock).toHaveBeenCalledWith('z1', 'lib-1'))
    confirmSpy.mockRestore()
  })

  it('delete is skipped when user cancels the confirm', async () => {
    listMock.mockResolvedValue({ entries: [sample()] })
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(false)
    render(<SkillsLibraryTab zoneId="z1" />)
    const row = (await screen.findByText('PDF Tools')).closest('tr')!
    fireEvent.click(within(row).getByLabelText(/delete pdf-tools/i))
    expect(removeMock).not.toHaveBeenCalled()
    confirmSpy.mockRestore()
  })
})

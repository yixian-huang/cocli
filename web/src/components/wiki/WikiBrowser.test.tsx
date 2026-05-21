import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { useUserStore } from '@/stores/userStore'
import { WikiBrowser } from './WikiBrowser'

const listPagesMock = vi.fn()
const getPageMock = vi.fn()
const listRevisionsMock = vi.fn()
const revertPageMock = vi.fn()
const listBacklinksMock = vi.fn()

vi.mock('@/lib/api/wiki', () => ({
  listPages: (...args: unknown[]) => listPagesMock(...args),
  getPage: (...args: unknown[]) => getPageMock(...args),
  listRevisions: (...args: unknown[]) => listRevisionsMock(...args),
  revertPage: (...args: unknown[]) => revertPageMock(...args),
  listBacklinks: (...args: unknown[]) => listBacklinksMock(...args),
}))

const now = new Date('2026-04-25T01:00:00.000Z').toISOString()

function asAdmin() {
  useUserStore.setState({
    user: {
      id: 'admin-id',
      name: 'admin',
      role: 'admin',
      hasPassword: true,
      createdAt: now,
    },
  })
}

describe('WikiBrowser', () => {
  beforeEach(() => {
    asAdmin()
    listPagesMock.mockReset()
    getPageMock.mockReset()
    listRevisionsMock.mockReset()
    revertPageMock.mockReset()
    listBacklinksMock.mockReset()
    // Default: no backlinks. Tests that care about backlinks override this.
    listBacklinksMock.mockResolvedValue({ backlinks: [] })
  })

  afterEach(() => {
    cleanup()
  })

  it('renders wiki list and loads the most recently updated page', async () => {
    listPagesMock.mockResolvedValue({
      pages: [
        { path: 'older', title: 'Older Page', tags: [], updatedAt: '2026-04-24T01:00:00.000Z' },
        { path: 'latest', title: 'Latest Page', tags: [], updatedAt: now },
      ],
    })
    getPageMock.mockImplementation(async (path: string) => ({
      path,
      title: path === 'latest' ? 'Latest Page' : 'Older Page',
      tags: [],
      updatedAt: now,
      content: path === 'latest' ? 'Latest content' : 'Older content',
    }))
    listRevisionsMock.mockResolvedValue({ revisions: [] })

    render(<WikiBrowser />)

    expect(await screen.findByText('Latest Page')).toBeInTheDocument()
    await waitFor(() => expect(getPageMock).toHaveBeenCalledWith('latest'))
    expect(await screen.findByText('Latest content')).toBeInTheDocument()
  })

  it('re-queries list when search changes', async () => {
    listPagesMock.mockResolvedValue({
      pages: [{ path: 'latest', title: 'Latest Page', tags: [], updatedAt: now }],
    })
    getPageMock.mockResolvedValue({
      path: 'latest',
      title: 'Latest Page',
      tags: [],
      updatedAt: now,
      content: 'Latest content',
    })
    listRevisionsMock.mockResolvedValue({ revisions: [] })

    render(<WikiBrowser />)
    await waitFor(() => expect(listPagesMock).toHaveBeenCalled())

    listPagesMock.mockClear()
    fireEvent.change(screen.getByPlaceholderText('title/path keyword'), { target: { value: 'proto' } })

    await waitFor(() => {
      expect(listPagesMock).toHaveBeenLastCalledWith({ q: 'proto', tag: undefined })
    })
  })

  it('switches displayed content when selecting a revision', async () => {
    listPagesMock.mockResolvedValue({
      pages: [{ path: 'latest', title: 'Latest Page', tags: [], updatedAt: now }],
    })
    getPageMock.mockResolvedValue({
      path: 'latest',
      title: 'Latest Page',
      tags: [],
      updatedAt: now,
      content: 'Current content',
    })
    listRevisionsMock.mockResolvedValue({
      revisions: [{ version: 2, content: 'Old revision content', createdAt: now }],
    })

    render(<WikiBrowser />)

    expect(await screen.findByText('Current content')).toBeInTheDocument()
    fireEvent.change(screen.getByRole('combobox', { name: 'Revision' }), { target: { value: '2' } })

    expect(await screen.findByText('Old revision content')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Edit' })).toBeDisabled()
  })

  it('renders the Referenced by section when backlinks are present', async () => {
    listPagesMock.mockResolvedValue({
      pages: [{ path: 'playbook', title: 'Playbook', tags: [], updatedAt: now }],
    })
    getPageMock.mockResolvedValue({
      path: 'playbook',
      title: 'Playbook',
      tags: [],
      updatedAt: now,
      content: 'Body',
    })
    listRevisionsMock.mockResolvedValue({ revisions: [] })
    listBacklinksMock.mockResolvedValue({
      backlinks: [
        { path: 'incident-2026-04', title: 'Incident 2026-04', updatedAt: now, version: 3 },
      ],
    })

    render(<WikiBrowser />)

    await waitFor(() => expect(listBacklinksMock).toHaveBeenCalledWith('playbook'))
    expect(await screen.findByLabelText('Backlinks')).toBeInTheDocument()
    expect(await screen.findByText('[[Incident 2026-04]]')).toBeInTheDocument()
  })

  it('shows empty-state copy when no backlinks exist', async () => {
    listPagesMock.mockResolvedValue({
      pages: [{ path: 'orphan', title: 'Orphan', tags: [], updatedAt: now }],
    })
    getPageMock.mockResolvedValue({
      path: 'orphan',
      title: 'Orphan',
      tags: [],
      updatedAt: now,
      content: 'Body',
    })
    listRevisionsMock.mockResolvedValue({ revisions: [] })
    listBacklinksMock.mockResolvedValue({ backlinks: [] })

    render(<WikiBrowser />)

    await waitFor(() => expect(listBacklinksMock).toHaveBeenCalledWith('orphan'))
    expect(await screen.findByText('No pages link here.')).toBeInTheDocument()
  })

  it('opens revert confirmation and calls revert API for selected revision', async () => {
    listPagesMock.mockResolvedValue({
      pages: [{ path: 'latest', title: 'Latest Page', tags: [], updatedAt: now }],
    })
    getPageMock.mockResolvedValue({
      path: 'latest',
      title: 'Latest Page',
      tags: [],
      updatedAt: now,
      content: 'Current content',
    })
    listRevisionsMock.mockResolvedValue({
      revisions: [{ version: 2, content: 'Old revision content', createdAt: now }],
    })
    revertPageMock.mockResolvedValue({
      path: 'latest',
      title: 'Latest Page',
      tags: [],
      updatedAt: now,
      content: 'Reverted head content',
    })

    render(<WikiBrowser />)
    await screen.findByText('Current content')

    fireEvent.change(screen.getByRole('combobox', { name: 'Revision' }), { target: { value: '2' } })
    fireEvent.click(await screen.findByRole('button', { name: 'Revert' }))

    expect(await screen.findByText('Revert wiki page?')).toBeInTheDocument()
    const revertButtons = screen.getAllByRole('button', { name: 'Revert' })
    fireEvent.click(revertButtons[revertButtons.length - 1])

    await waitFor(() => expect(revertPageMock).toHaveBeenCalledWith('latest', 2))
    expect(await screen.findByText('Reverted head content')).toBeInTheDocument()
  })
})

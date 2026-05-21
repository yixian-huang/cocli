import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { SkillsLibraryImportModal } from './SkillsLibraryImportModal'

const importMock = vi.fn()
const reinstallMock = vi.fn()

vi.mock('@/api/client', () => ({
  zoneSkillLibrary: {
    import: (...a: unknown[]) => importMock(...a),
    reinstall: (...a: unknown[]) => reinstallMock(...a),
  },
  ApiError: class ApiError extends Error {
    status: number; body?: string; requestId = 'rid'
    constructor(m: string, s: number, b?: string) { super(m); this.status = s; this.body = b }
  },
}))

beforeEach(() => {
  importMock.mockReset()
  reinstallMock.mockReset()
})
afterEach(() => cleanup())

describe('SkillsLibraryImportModal', () => {
  it('disables Import button until URL is entered', () => {
    render(
      <SkillsLibraryImportModal open zoneId="z1" onClose={() => {}} onImported={() => {}} />
    )
    expect(screen.getByRole('button', { name: /^import$/i })).toBeDisabled()
    fireEvent.change(screen.getByLabelText(/^url$/i), { target: { value: 'https://x/y' } })
    expect(screen.getByRole('button', { name: /^import$/i })).not.toBeDisabled()
  })

  it('successful import calls onImported and closes', async () => {
    importMock.mockResolvedValue({ library_id: 'lib-1', files: 5, size: 12345 })
    const onImported = vi.fn()
    render(
      <SkillsLibraryImportModal open zoneId="z1" onClose={() => {}} onImported={onImported} />
    )
    fireEvent.change(screen.getByLabelText(/^url$/i), { target: { value: 'https://x/y' } })
    fireEvent.click(screen.getByRole('button', { name: /^import$/i }))
    await waitFor(() => expect(onImported).toHaveBeenCalledWith('lib-1'))
  })

  it('shows progress label and a Cancel button while in flight', async () => {
    let resolveImport!: (v: unknown) => void
    importMock.mockReturnValue(new Promise((res) => { resolveImport = res }))
    render(
      <SkillsLibraryImportModal open zoneId="z1" onClose={() => {}} onImported={() => {}} />
    )
    fireEvent.change(screen.getByLabelText(/^url$/i), { target: { value: 'https://x/y' } })
    fireEvent.click(screen.getByRole('button', { name: /^import$/i }))
    expect(await screen.findByText(/importing/i)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /^cancel$/i })).toBeInTheDocument()
    resolveImport({ library_id: 'lib-1', files: 1, size: 100 })
  })

  it('Cancel button aborts the in-flight request', async () => {
    let abortSeen: AbortSignal | undefined
    importMock.mockImplementation((_z: string, _b: unknown, opts?: { signal?: AbortSignal }) => {
      abortSeen = opts?.signal
      return new Promise((_res, rej) => {
        opts?.signal?.addEventListener('abort', () => rej(new DOMException('aborted', 'AbortError')))
      })
    })
    render(
      <SkillsLibraryImportModal open zoneId="z1" onClose={() => {}} onImported={() => {}} />
    )
    fireEvent.change(screen.getByLabelText(/^url$/i), { target: { value: 'https://x/y' } })
    fireEvent.click(screen.getByRole('button', { name: /^import$/i }))
    fireEvent.click(await screen.findByRole('button', { name: /^cancel$/i }))
    await waitFor(() => expect(abortSeen?.aborted).toBe(true))
  })

  it('409 conflict offers Reinstall path', async () => {
    const err = Object.assign(new Error('name already imported'), {
      status: 409,
      body: JSON.stringify({ existing_source: 'https://old/url', existing_id: 'lib-old' }),
    })
    importMock.mockRejectedValue(err)
    reinstallMock.mockResolvedValue({ updated: true, source_ref: 'abc1234' })
    const onImported = vi.fn()
    render(
      <SkillsLibraryImportModal open zoneId="z1" onClose={() => {}} onImported={onImported} />
    )
    fireEvent.change(screen.getByLabelText(/^url$/i), { target: { value: 'https://x/y' } })
    fireEvent.click(screen.getByRole('button', { name: /^import$/i }))
    expect(await screen.findByText(/already exists/i)).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: /reinstall/i }))
    await waitFor(() => expect(reinstallMock).toHaveBeenCalledWith('z1', 'lib-old'))
    await waitFor(() => expect(onImported).toHaveBeenCalledWith('lib-old'))
  })
})

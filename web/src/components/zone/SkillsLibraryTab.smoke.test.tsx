import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { SkillsLibraryTab } from './SkillsLibraryTab'

vi.mock('@/api/client', () => ({
  zoneSkillLibrary: {
    list: vi.fn().mockResolvedValue({ entries: [] }),
  },
}))

afterEach(() => cleanup())

describe('SkillsLibraryTab smoke', () => {
  it('renders the toolbar with Import URL button', async () => {
    render(<SkillsLibraryTab zoneId="z1" />)
    expect(await screen.findByRole('button', { name: /import url/i })).toBeInTheDocument()
  })

  it('shows empty state when zero entries', async () => {
    render(<SkillsLibraryTab zoneId="z1" />)
    expect(await screen.findByText(/no skills imported yet/i)).toBeInTheDocument()
  })
})

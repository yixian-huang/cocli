import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { ZoneSettingsView } from './ZoneSettingsView'

vi.mock('./SkillsLibraryTab', () => ({
  SkillsLibraryTab: ({ zoneId }: { zoneId: string }) => (
    <div data-testid="library-tab">zone={zoneId}</div>
  ),
}))

afterEach(() => cleanup())

describe('ZoneSettingsView', () => {
  it('renders Skills Library tab by default', () => {
    render(
      <MemoryRouter>
        <ZoneSettingsView zoneId="z1" />
      </MemoryRouter>
    )
    expect(screen.getByRole('button', { name: /skills library/i })).toBeInTheDocument()
    expect(screen.getByTestId('library-tab')).toHaveTextContent('zone=z1')
  })

  it('passes zoneId through to the active tab', () => {
    render(
      <MemoryRouter>
        <ZoneSettingsView zoneId="z-abc" />
      </MemoryRouter>
    )
    expect(screen.getByTestId('library-tab')).toHaveTextContent('zone=z-abc')
  })

  it('clicking the tab keeps it active (no crash on re-select)', () => {
    render(
      <MemoryRouter>
        <ZoneSettingsView zoneId="z1" />
      </MemoryRouter>
    )
    fireEvent.click(screen.getByRole('button', { name: /skills library/i }))
    expect(screen.getByTestId('library-tab')).toBeInTheDocument()
  })
})

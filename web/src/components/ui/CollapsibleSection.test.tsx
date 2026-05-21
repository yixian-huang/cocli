import { describe, expect, it, beforeEach, vi, afterEach } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import { CollapsibleSection } from './CollapsibleSection'
import { resetPrefsStore, getCollapsed, setCollapsed } from '@/stores/prefsStore'
import * as client from '@/api/client'

describe('<CollapsibleSection>', () => {
  beforeEach(() => {
    vi.spyOn(client.prefs, 'put').mockResolvedValue({ ok: true })
    resetPrefsStore()
  })
  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('renders children when expanded and unmounts them when collapsed', () => {
    render(
      <CollapsibleSection id="t.x" title="Things">
        <div data-testid="child">child</div>
      </CollapsibleSection>
    )
    expect(screen.getByTestId('child')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: /things/i }))
    expect(screen.queryByTestId('child')).toBeNull()
  })

  it('persists collapse state through prefsStore', () => {
    render(
      <CollapsibleSection id="t.persist" title="Persist">
        <div>x</div>
      </CollapsibleSection>
    )
    fireEvent.click(screen.getByRole('button', { name: /persist/i }))
    expect(getCollapsed('t.persist')).toBe(true)
  })

  it('respects pre-existing prefs state', () => {
    setCollapsed('t.preset', true)
    render(
      <CollapsibleSection id="t.preset" title="Preset">
        <div data-testid="child">x</div>
      </CollapsibleSection>
    )
    expect(screen.queryByTestId('child')).toBeNull()
  })

  it('Enter and Space toggle the header button', () => {
    render(
      <CollapsibleSection id="t.kbd" title="Kbd">
        <div data-testid="child">x</div>
      </CollapsibleSection>
    )
    const btn = screen.getByRole('button', { name: /kbd/i })
    btn.focus()
    fireEvent.keyDown(btn, { key: 'Enter' })
    expect(screen.queryByTestId('child')).toBeNull()
    fireEvent.keyDown(btn, { key: ' ' })
    expect(screen.queryByTestId('child')).not.toBeNull()
  })
})

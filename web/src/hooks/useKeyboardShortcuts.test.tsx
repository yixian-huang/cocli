import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { useKeyboardShortcuts, resetKeyboardShortcutsForTests, type ShortcutDefinition } from './useKeyboardShortcuts'

function ShortcutHarness({ shortcuts }: { shortcuts: ShortcutDefinition[] }) {
  useKeyboardShortcuts(shortcuts)
  return <input aria-label="editor" />
}

describe('useKeyboardShortcuts', () => {
  afterEach(() => {
    cleanup()
    resetKeyboardShortcutsForTests()
    vi.restoreAllMocks()
  })

  it('prefers the highest priority registered shortcut', () => {
    const low = vi.fn()
    const high = vi.fn()

    render(
      <>
        <ShortcutHarness shortcuts={[{ key: 'escape', priority: 10, handler: low }]} />
        <ShortcutHarness shortcuts={[{ key: 'escape', priority: 100, handler: high }]} />
      </>,
    )

    fireEvent.keyDown(window, { key: 'Escape' })

    expect(high).toHaveBeenCalledTimes(1)
    expect(low).not.toHaveBeenCalled()
  })

  it('removes shortcuts on unmount', () => {
    const handler = vi.fn()

    const { unmount } = render(
      <ShortcutHarness shortcuts={[{ key: 'k', handler, mod: false, shift: false, alt: false }]} />,
    )

    unmount()
    fireEvent.keyDown(window, { key: 'k' })

    expect(handler).not.toHaveBeenCalled()
  })

  it('blocks shortcuts inside inputs unless allowInInput is enabled', () => {
    const blocked = vi.fn()
    const allowed = vi.fn()

    render(
      <ShortcutHarness
        shortcuts={[
          { key: '/', handler: blocked, mod: false, shift: false, alt: false },
          { key: 'escape', handler: allowed, allowInInput: true },
        ]}
      />,
    )

    const input = screen.getByLabelText('editor')
    input.focus()

    fireEvent.keyDown(input, { key: '/' })
    fireEvent.keyDown(input, { key: 'Escape' })

    expect(blocked).not.toHaveBeenCalled()
    expect(allowed).toHaveBeenCalledTimes(1)
  })
})

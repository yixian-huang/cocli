import { afterEach, describe, expect, it, vi } from 'vitest'
import type { HTMLAttributes, ReactNode } from 'react'
import { cleanup, render, screen } from '@testing-library/react'
import { ShortcutsOverlay } from './ShortcutsOverlay'
import { resetKeyboardShortcutsForTests } from '@/hooks/useKeyboardShortcuts'

vi.mock('framer-motion', () => ({
  AnimatePresence: ({ children }: { children: ReactNode }) => <>{children}</>,
  motion: {
    div: ({ children, ...props }: HTMLAttributes<HTMLDivElement>) => <div {...props}>{children}</div>,
  },
}))

describe('ShortcutsOverlay', () => {
  afterEach(() => {
    cleanup()
    resetKeyboardShortcutsForTests()
  })

  it('shows and hides the shortcuts help overlay', () => {
    const sections = [
      {
        title: 'Navigation',
        items: [{ keys: ['⌘ K'], description: 'Open the channel switcher' }],
      },
    ]

    const { rerender } = render(
      <ShortcutsOverlay open onClose={vi.fn()} sections={sections} />,
    )

    expect(screen.getByTestId('shortcuts-overlay')).toBeInTheDocument()
    expect(screen.getByText('Open the channel switcher')).toBeInTheDocument()

    rerender(<ShortcutsOverlay open={false} onClose={vi.fn()} sections={sections} />)

    expect(screen.queryByTestId('shortcuts-overlay')).not.toBeInTheDocument()
  })
})

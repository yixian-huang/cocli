import { describe, it, expect, afterEach } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { ContextBar } from './ContextBar'

afterEach(() => cleanup())

describe('ContextBar variants', () => {
  const props = {
    lastInputTokens: 6200,
    contextWindow: 10000,
    totalOutputTokens: 1200,
    totalCostUSD: 0.42,
    turnCount: 12,
  }

  it('renders the full variant by default with threshold markers and metrics row', () => {
    const { container } = render(<ContextBar {...props} />)
    expect(screen.getByText(/62% context/)).toBeInTheDocument()
    expect(screen.getByText(/Moderate|High|Normal/)).toBeInTheDocument()
    expect(screen.getByText(/12 turns/)).toBeInTheDocument()
    expect(screen.getByText(/\$0\.420/)).toBeInTheDocument()
    expect(container.querySelectorAll('[title^="L"]').length).toBe(3)
  })

  it('inline variant renders a single compact row without threshold markers', () => {
    const { container } = render(<ContextBar {...props} variant="inline" />)
    expect(screen.getByTestId('context-bar-inline')).toBeInTheDocument()
    expect(screen.getByText(/62%/)).toBeInTheDocument()
    expect(screen.getByText(/12 turns/)).toBeInTheDocument()
    expect(screen.getByText(/\$0\.42/)).toBeInTheDocument()
    expect(container.querySelectorAll('[title^="L"]').length).toBe(0)
  })

  it('returns null when context data is missing in either variant', () => {
    const { container: c1 } = render(<ContextBar variant="full" />)
    const { container: c2 } = render(<ContextBar variant="inline" />)
    expect(c1.firstChild).toBeNull()
    expect(c2.firstChild).toBeNull()
  })
})

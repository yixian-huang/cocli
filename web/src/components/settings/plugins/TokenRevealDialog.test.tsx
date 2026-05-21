import { describe, expect, it, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { TokenRevealDialog } from './TokenRevealDialog'

describe('<TokenRevealDialog>', () => {
  it('renders nothing when token=null', () => {
    const { container } = render(<TokenRevealDialog token={null} onClose={() => {}} />)
    expect(container.firstChild).toBeNull()
  })

  it('renders the token in a monospace box + Copy button + warning + Done', () => {
    render(<TokenRevealDialog token="abc-123-def" onClose={() => {}} />)
    expect(screen.getByText('abc-123-def')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /copy/i })).toBeInTheDocument()
    expect(screen.getByText(/won't be shown again/i)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /i've saved it/i })).toBeInTheDocument()
  })

  it('Copy button writes the token to the clipboard', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined)
    Object.assign(navigator, { clipboard: { writeText } })
    render(<TokenRevealDialog token="abc-123" onClose={() => {}} />)
    fireEvent.click(screen.getByRole('button', { name: /copy/i }))
    expect(writeText).toHaveBeenCalledWith('abc-123')
  })

  it('I\'ve saved it button calls onClose', () => {
    const onClose = vi.fn()
    render(<TokenRevealDialog token="abc" onClose={onClose} />)
    fireEvent.click(screen.getByRole('button', { name: /i've saved it/i }))
    expect(onClose).toHaveBeenCalledOnce()
  })
})

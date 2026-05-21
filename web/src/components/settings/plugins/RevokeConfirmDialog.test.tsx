import { describe, expect, it, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { RevokeConfirmDialog } from './RevokeConfirmDialog'

const sample = {
  id: 'p1', name: 'telegram-bot',
  capabilities: ['inbound-bridge' as const],
  createdAt: '2026-05-21T00:00:00Z', lastSeenAt: null,
}

describe('<RevokeConfirmDialog>', () => {
  it('renders nothing when plugin=null', () => {
    const { container } = render(<RevokeConfirmDialog plugin={null} onClose={() => {}} onConfirm={() => {}} />)
    expect(container.firstChild).toBeNull()
  })

  it('renders body with plugin name and both buttons', () => {
    render(<RevokeConfirmDialog plugin={{ ...sample, capabilities: [...sample.capabilities] }} onClose={() => {}} onConfirm={() => {}} />)
    expect(screen.getByText(/telegram-bot/i)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /cancel/i })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /revoke/i })).toBeInTheDocument()
  })

  it('Revoke calls onConfirm; Cancel calls onClose', () => {
    const onConfirm = vi.fn()
    const onClose = vi.fn()
    render(<RevokeConfirmDialog plugin={{ ...sample, capabilities: [...sample.capabilities] }} onClose={onClose} onConfirm={onConfirm} />)
    fireEvent.click(screen.getByRole('button', { name: /revoke/i }))
    expect(onConfirm).toHaveBeenCalledOnce()
    fireEvent.click(screen.getByRole('button', { name: /cancel/i }))
    expect(onClose).toHaveBeenCalledOnce()
  })
})

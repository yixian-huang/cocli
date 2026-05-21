import { describe, expect, it, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { PluginRow } from './PluginRow'

const sample = {
  id: 'p1',
  name: 'telegram-bot',
  capabilities: ['inbound-bridge', 'outbound-bridge'] as const,
  createdAt: '2026-05-18T00:00:00Z',
  lastSeenAt: null,
}

describe('<PluginRow>', () => {
  it('renders plugin name + capability badges', () => {
    render(<ul><PluginRow plugin={{ ...sample, capabilities: [...sample.capabilities] }} onRevoke={() => {}} /></ul>)
    expect(screen.getByText('telegram-bot')).toBeInTheDocument()
    expect(screen.getByText('inbound-bridge')).toBeInTheDocument()
    expect(screen.getByText('outbound-bridge')).toBeInTheDocument()
  })

  it('renders "Last seen: never" when lastSeenAt is null', () => {
    render(<ul><PluginRow plugin={{ ...sample, capabilities: [...sample.capabilities] }} onRevoke={() => {}} /></ul>)
    expect(screen.getByText(/last seen: never/i)).toBeInTheDocument()
  })

  it('trash button calls onRevoke', () => {
    const onRevoke = vi.fn()
    render(<ul><PluginRow plugin={{ ...sample, capabilities: [...sample.capabilities] }} onRevoke={onRevoke} /></ul>)
    fireEvent.click(screen.getByRole('button', { name: /revoke/i }))
    expect(onRevoke).toHaveBeenCalledOnce()
  })
})

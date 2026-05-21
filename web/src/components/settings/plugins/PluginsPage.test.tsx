import { describe, expect, it, beforeEach } from 'vitest'
import { render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { PluginsPage } from './PluginsPage'
import { usePluginsStore } from '@/stores/pluginsStore'

beforeEach(() => {
  localStorage.clear()
  usePluginsStore.setState({ plugins: [] })
})

function r() {
  return render(<MemoryRouter><PluginsPage /></MemoryRouter>)
}

describe('<PluginsPage>', () => {
  it('renders header Plugins + Register button', () => {
    r()
    expect(screen.getByRole('heading', { name: /plugins/i })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /register plugin/i })).toBeInTheDocument()
  })

  it('shows empty state when no plugins', () => {
    r()
    expect(screen.getByText(/No plugins yet/i)).toBeInTheDocument()
  })

  it('shows plugin rows when store has items', () => {
    usePluginsStore.setState({
      plugins: [{
        id: 'p1', name: 'telegram-bot',
        capabilities: ['inbound-bridge'],
        createdAt: '2026-05-21T00:00:00Z', lastSeenAt: null,
      }],
    })
    r()
    expect(screen.getByText('telegram-bot')).toBeInTheDocument()
    expect(screen.queryByText(/No plugins yet/i)).toBeNull()
  })
})

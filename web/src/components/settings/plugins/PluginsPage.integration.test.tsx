import { describe, expect, it, beforeEach } from 'vitest'
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { PluginsPage } from './PluginsPage'
import { usePluginsStore } from '@/stores/pluginsStore'

beforeEach(() => {
  localStorage.clear()
  usePluginsStore.setState({ plugins: [] })
})

describe('PluginsPage integration', () => {
  it('full register → reveal → close → revoke flow', async () => {
    render(<MemoryRouter><PluginsPage /></MemoryRouter>)

    expect(screen.getByText(/No plugins yet/i)).toBeInTheDocument()

    // Register
    fireEvent.click(screen.getByRole('button', { name: /register plugin/i }))
    fireEvent.change(screen.getByLabelText(/plugin name/i), { target: { value: 'telegram-bot' } })
    fireEvent.click(screen.getByLabelText(/inbound-bridge/i))
    // The dialog has the "Register" button in the footer
    const registerDialog = screen.getByRole('dialog')
    fireEvent.click(within(registerDialog).getByRole('button', { name: /^Register$/i }))

    // Token reveal
    const reveal = await screen.findByText(/won't be shown again/i)
    expect(reveal).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: /i've saved it/i }))

    // Row appears
    await waitFor(() => expect(screen.getByText('telegram-bot')).toBeInTheDocument())
    expect(usePluginsStore.getState().plugins).toHaveLength(1)

    // Revoke: click the row's revoke icon button, then the destructive confirm in dialog
    fireEvent.click(screen.getByRole('button', { name: /revoke/i }))
    // The dialog now has a "Revoke" button — use within to select from dialog
    const dialog = screen.getByRole('dialog')
    fireEvent.click(within(dialog).getByRole('button', { name: /revoke/i }))

    await waitFor(() => expect(screen.queryByText('telegram-bot')).toBeNull())
    expect(usePluginsStore.getState().plugins).toHaveLength(0)
  })
})

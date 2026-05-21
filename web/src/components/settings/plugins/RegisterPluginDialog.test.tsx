import { describe, expect, it, beforeEach, vi } from 'vitest'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { RegisterPluginDialog } from './RegisterPluginDialog'
import { usePluginsStore } from '@/stores/pluginsStore'

beforeEach(() => {
  localStorage.clear()
  usePluginsStore.setState({ plugins: [] })
})

describe('<RegisterPluginDialog>', () => {
  it('renders nothing when open=false', () => {
    const { container } = render(
      <RegisterPluginDialog open={false} onClose={() => {}} onRegistered={() => {}} />,
    )
    expect(container.firstChild).toBeNull()
  })

  it('renders name input + both capability checkboxes + Register button', () => {
    render(<RegisterPluginDialog open={true} onClose={() => {}} onRegistered={() => {}} />)
    expect(screen.getByLabelText(/plugin name/i)).toBeInTheDocument()
    expect(screen.getByLabelText(/inbound-bridge/i)).toBeInTheDocument()
    expect(screen.getByLabelText(/outbound-bridge/i)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /register/i })).toBeInTheDocument()
  })

  it('Register is disabled until name + at least one capability', () => {
    render(<RegisterPluginDialog open={true} onClose={() => {}} onRegistered={() => {}} />)
    const submit = screen.getByRole('button', { name: /register/i })
    expect(submit).toBeDisabled()

    fireEvent.change(screen.getByLabelText(/plugin name/i), { target: { value: 'tg' } })
    expect(submit).toBeDisabled()

    fireEvent.click(screen.getByLabelText(/inbound-bridge/i))
    expect(submit).not.toBeDisabled()
  })

  it('submitting calls store.register and onRegistered with the plaintext token', async () => {
    const onRegistered = vi.fn()
    render(<RegisterPluginDialog open={true} onClose={() => {}} onRegistered={onRegistered} />)
    fireEvent.change(screen.getByLabelText(/plugin name/i), { target: { value: 'telegram-bot' } })
    fireEvent.click(screen.getByLabelText(/inbound-bridge/i))
    fireEvent.click(screen.getByRole('button', { name: /register/i }))
    await waitFor(() => expect(onRegistered).toHaveBeenCalled())
    const [token] = onRegistered.mock.calls[0]
    expect(token).toMatch(/^[0-9a-f-]{36}$/)
    expect(usePluginsStore.getState().plugins).toHaveLength(1)
  })

  it('Cancel calls onClose without registering', () => {
    const onClose = vi.fn()
    render(<RegisterPluginDialog open={true} onClose={onClose} onRegistered={() => {}} />)
    fireEvent.click(screen.getByRole('button', { name: /cancel/i }))
    expect(onClose).toHaveBeenCalledOnce()
    expect(usePluginsStore.getState().plugins).toHaveLength(0)
  })
})

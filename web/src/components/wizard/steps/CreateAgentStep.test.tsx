import { describe, expect, it, beforeEach } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { CreateAgentStep } from './CreateAgentStep'
import { useWizardStore } from '@/stores/wizardStore'

beforeEach(() => {
  localStorage.clear()
  useWizardStore.setState({
    step: 2, complete: false, claudePath: '', detectedAt: null,
    draftAgent: { name: '', model: 'claude-sonnet-4-6' },
  })
})

describe('<CreateAgentStep>', () => {
  it('renders name + model + Back + Next', () => {
    render(<CreateAgentStep />)
    expect(screen.getByLabelText(/agent name/i)).toBeInTheDocument()
    expect(screen.getByLabelText(/model/i)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /back/i })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /next/i })).toBeInTheDocument()
  })

  it('Next is disabled when name is empty', () => {
    render(<CreateAgentStep />)
    expect(screen.getByRole('button', { name: /next/i })).toBeDisabled()
  })

  it('typing a valid name enables Next and updates store', () => {
    render(<CreateAgentStep />)
    fireEvent.change(screen.getByLabelText(/agent name/i), { target: { value: '@assistant' } })
    expect(useWizardStore.getState().draftAgent.name).toBe('@assistant')
    expect(screen.getByRole('button', { name: /next/i })).not.toBeDisabled()
  })

  it('auto-prepends @ when typing a bare name', () => {
    render(<CreateAgentStep />)
    fireEvent.change(screen.getByLabelText(/agent name/i), { target: { value: 'helper' } })
    expect(useWizardStore.getState().draftAgent.name).toBe('@helper')
  })

  it('strips invalid characters', () => {
    render(<CreateAgentStep />)
    fireEvent.change(screen.getByLabelText(/agent name/i), { target: { value: '@HAS SPACE' } })
    expect(useWizardStore.getState().draftAgent.name).toBe('@hasspace')
  })

  it('changing model updates store', () => {
    render(<CreateAgentStep />)
    fireEvent.change(screen.getByLabelText(/model/i), { target: { value: 'claude-haiku-4-5' } })
    expect(useWizardStore.getState().draftAgent.model).toBe('claude-haiku-4-5')
  })

  it('Back returns to step 1', () => {
    render(<CreateAgentStep />)
    fireEvent.click(screen.getByRole('button', { name: /back/i }))
    expect(useWizardStore.getState().step).toBe(1)
  })

  it('Next advances to step 3', () => {
    useWizardStore.setState({ draftAgent: { name: '@a', model: 'claude-sonnet-4-6' } })
    render(<CreateAgentStep />)
    fireEvent.click(screen.getByRole('button', { name: /next/i }))
    expect(useWizardStore.getState().step).toBe(3)
  })
})

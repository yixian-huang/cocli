import { describe, expect, it, beforeEach } from 'vitest'
import { render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { FirstRunWizard } from './FirstRunWizard'
import { useWizardStore } from '@/stores/wizardStore'

beforeEach(() => {
  localStorage.clear()
  useWizardStore.setState({
    step: 1, complete: false, claudePath: '', detectedAt: null,
    draftAgent: { name: '', model: 'claude-sonnet-4-6' },
  })
})

function renderWiz() {
  return render(<MemoryRouter><FirstRunWizard /></MemoryRouter>)
}

describe('<FirstRunWizard>', () => {
  it('renders the headline "Welcome to cocli local"', () => {
    renderWiz()
    expect(screen.getByText(/Welcome to cocli local/i)).toBeInTheDocument()
  })

  it('renders 3 progress dots and highlights step 1', () => {
    renderWiz()
    const dots = screen.getAllByTestId('wizard-progress-dot')
    expect(dots).toHaveLength(3)
    expect(dots[0]).toHaveAttribute('data-active', 'true')
    expect(dots[1]).toHaveAttribute('data-active', 'false')
  })

  it('does not render when complete=true', () => {
    useWizardStore.setState({ complete: true })
    const { container } = renderWiz()
    expect(container.firstChild).toBeNull()
  })

  it('renders LocateClaudeStep at step 1', () => {
    renderWiz()
    expect(screen.getByText(/Where is your Claude CLI/i)).toBeInTheDocument()
  })

  it('renders CreateAgentStep at step 2', () => {
    useWizardStore.setState({ step: 2 })
    renderWiz()
    expect(screen.getByText(/Create your first agent/i)).toBeInTheDocument()
  })

  it('renders TryItStep at step 3', () => {
    useWizardStore.setState({ step: 3 })
    renderWiz()
    expect(screen.getByText(/You're all set/i)).toBeInTheDocument()
  })
})

import { describe, expect, it, beforeEach, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { MemoryRouter, useNavigate } from 'react-router-dom'
import { TryItStep } from './TryItStep'
import { useWizardStore } from '@/stores/wizardStore'

vi.mock('react-router-dom', async () => {
  const actual = await vi.importActual<typeof import('react-router-dom')>('react-router-dom')
  return { ...actual, useNavigate: vi.fn() }
})

beforeEach(() => {
  localStorage.clear()
  useWizardStore.setState({
    step: 3, complete: false, claudePath: '', detectedAt: null,
    draftAgent: { name: '@assistant', model: 'claude-sonnet-4-6' },
  })
  vi.mocked(useNavigate).mockReturnValue(vi.fn())
})

function renderStep() {
  return render(<MemoryRouter><TryItStep /></MemoryRouter>)
}

describe('<TryItStep>', () => {
  it('renders headline + Open #general + Maybe later', () => {
    renderStep()
    expect(screen.getByText(/You're all set/i)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /open #general/i })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /maybe later/i })).toBeInTheDocument()
  })

  it('Open #general calls finish() and navigates', () => {
    const navigate = vi.fn()
    vi.mocked(useNavigate).mockReturnValue(navigate)
    renderStep()
    fireEvent.click(screen.getByRole('button', { name: /open #general/i }))
    expect(useWizardStore.getState().complete).toBe(true)
    expect(navigate).toHaveBeenCalledWith('/channel/general')
  })

  it('Maybe later calls finish() but does not navigate', () => {
    const navigate = vi.fn()
    vi.mocked(useNavigate).mockReturnValue(navigate)
    renderStep()
    fireEvent.click(screen.getByRole('button', { name: /maybe later/i }))
    expect(useWizardStore.getState().complete).toBe(true)
    expect(navigate).not.toHaveBeenCalled()
  })
})

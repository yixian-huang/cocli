import { describe, expect, it, beforeEach, vi } from 'vitest'
import { render, screen, fireEvent, act } from '@testing-library/react'
import { LocateClaudeStep } from './LocateClaudeStep'
import { useWizardStore } from '@/stores/wizardStore'

beforeEach(() => {
  localStorage.clear()
  useWizardStore.setState({
    step: 1, complete: false, claudePath: '', detectedAt: null,
    draftAgent: { name: '', model: 'claude-sonnet-4-6' },
  })
})

describe('<LocateClaudeStep>', () => {
  it('renders headline + path input + Detect button + Next', () => {
    render(<LocateClaudeStep />)
    expect(screen.getByLabelText(/path to claude/i)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /detect/i })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /next/i })).toBeInTheDocument()
  })

  it('typing updates claudePath in store', () => {
    render(<LocateClaudeStep />)
    fireEvent.change(screen.getByLabelText(/path to claude/i), {
      target: { value: '/opt/claude/bin/claude' },
    })
    expect(useWizardStore.getState().claudePath).toBe('/opt/claude/bin/claude')
  })

  it('clicking Detect shows a check on success', async () => {
    vi.useFakeTimers()
    render(<LocateClaudeStep />)
    fireEvent.click(screen.getByRole('button', { name: /detect/i }))
    await act(async () => { vi.advanceTimersByTime(700) })
    expect(screen.getByTestId('detect-success')).toBeInTheDocument()
    vi.useRealTimers()
  })

  it('Next advances to step 2 even with empty path', () => {
    render(<LocateClaudeStep />)
    fireEvent.click(screen.getByRole('button', { name: /next/i }))
    expect(useWizardStore.getState().step).toBe(2)
  })
})

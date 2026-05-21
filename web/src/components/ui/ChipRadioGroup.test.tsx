import { describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import { ChipRadioGroup, type ChipRadioOption } from './ChipRadioGroup'

type Urgency = 'low' | 'normal' | 'high' | 'critical'

const options: ChipRadioOption<Urgency>[] = [
  { value: 'normal', label: 'normal', description: 'Default delivery.', tone: 'info' },
  { value: 'low', label: 'low', description: 'Can wait.', tone: 'neutral' },
  { value: 'high', label: 'high', description: 'Needs prompt attention.', tone: 'warn' },
  { value: 'critical', label: 'critical', description: 'Interrupt-worthy.', tone: 'danger' },
]

describe('ChipRadioGroup', () => {
  it('renders the selected value and notifies on changes', () => {
    const onChange = vi.fn()
    render(
      <ChipRadioGroup
        aria-label="Urgency"
        value="normal"
        onChange={onChange}
        options={options}
      />,
    )

    expect(screen.getByRole('radio', { name: 'normal' })).toHaveAttribute('aria-checked', 'true')
    expect(screen.getByRole('radio', { name: 'critical' })).toHaveAttribute('title', 'Interrupt-worthy.')

    fireEvent.click(screen.getByRole('radio', { name: 'high' }))

    expect(onChange).toHaveBeenCalledWith('high')
  })

  it('disables every chip when the group is disabled', () => {
    const onChange = vi.fn()
    render(
      <ChipRadioGroup
        aria-label="Urgency"
        value="normal"
        onChange={onChange}
        options={options}
        disabled
      />,
    )

    const highChip = screen.getByRole('radio', { name: 'high' })
    expect(highChip).toBeDisabled()

    fireEvent.click(highChip)

    expect(onChange).not.toHaveBeenCalled()
  })
})

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import { ListFilter } from './ListFilter'

describe('<ListFilter>', () => {
  beforeEach(() => vi.useFakeTimers())
  afterEach(() => vi.useRealTimers())

  it('debounces onChange by 120ms', () => {
    const onChange = vi.fn()
    render(<ListFilter value="" onChange={onChange} />)
    const input = screen.getByRole('searchbox')
    fireEvent.change(input, { target: { value: 'a' } })
    fireEvent.change(input, { target: { value: 'al' } })
    fireEvent.change(input, { target: { value: 'alp' } })
    expect(onChange).not.toHaveBeenCalled()
    vi.advanceTimersByTime(120)
    expect(onChange).toHaveBeenCalledWith('alp')
    expect(onChange).toHaveBeenCalledTimes(1)
  })

  it('ESC clears and blurs', () => {
    const onChange = vi.fn()
    render(<ListFilter value="alpha" onChange={onChange} />)
    const input = screen.getByRole('searchbox') as HTMLInputElement
    input.focus()
    fireEvent.keyDown(input, { key: 'Escape' })
    vi.advanceTimersByTime(120)
    expect(onChange).toHaveBeenCalledWith('')
    expect(document.activeElement).not.toBe(input)
  })

  it('shows resultCount when provided', () => {
    render(<ListFilter value="x" onChange={() => {}} resultCount={3} totalCount={10} />)
    expect(screen.getByText(/3.*10/)).toBeInTheDocument()
  })
})

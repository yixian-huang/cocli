import { act } from 'react'
import { fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ToastContainer } from './Toast'
import { MAX_TOASTS, TOAST_EXIT_MS, resetToastStore, toast } from '@/stores/toastStore'

describe('ToastContainer', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    resetToastStore()
  })

  afterEach(() => {
    resetToastStore()
    vi.runOnlyPendingTimers()
    vi.useRealTimers()
  })

  it('renders only the latest four toasts when the queue overflows', () => {
    render(<ToastContainer />)

    act(() => {
      for (let i = 1; i <= MAX_TOASTS + 1; i += 1) {
        toast(`toast-${i}`)
      }
    })

    expect(screen.queryByText('toast-1')).not.toBeInTheDocument()
    expect(screen.getByText('toast-5')).toBeInTheDocument()
    expect(screen.getByText('toast-2')).toBeInTheDocument()
  })

  it('dismisses a toast when the close button is pressed', () => {
    render(<ToastContainer />)

    act(() => {
      toast('saved', 'success')
    })

    fireEvent.click(screen.getByRole('button', { name: /dismiss success notification/i }))

    act(() => {
      vi.advanceTimersByTime(TOAST_EXIT_MS)
    })

    expect(screen.queryByText('saved')).not.toBeInTheDocument()
  })

  it('dismisses the latest toast when escape is pressed', () => {
    render(<ToastContainer />)

    act(() => {
      toast('older', 'info')
      toast('newer', 'warn')
    })

    fireEvent.keyDown(window, { key: 'Escape' })

    act(() => {
      vi.advanceTimersByTime(TOAST_EXIT_MS)
    })

    expect(screen.queryByText('newer')).not.toBeInTheDocument()
    expect(screen.getByText('older')).toBeInTheDocument()
  })
})

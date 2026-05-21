import { describe, it, expect, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { Button } from '../Button'

describe('Button polish', () => {
  it('renders size=xs with 28px height utility', () => {
    render(<Button size="xs">Tiny</Button>)
    const btn = screen.getByRole('button', { name: 'Tiny' })
    expect(btn.className).toMatch(/h-7\b/)
  })

  it('disabled prevents onClick', () => {
    const onClick = vi.fn()
    render(<Button disabled onClick={onClick}>Off</Button>)
    fireEvent.click(screen.getByRole('button', { name: 'Off' }))
    expect(onClick).not.toHaveBeenCalled()
  })

  it('loading hides children and shows spinner', () => {
    render(<Button loading>Submit</Button>)
    expect(screen.queryByText('Submit')).not.toBeInTheDocument()
    expect(document.querySelector('svg.animate-spin')).not.toBeNull()
  })

  it('has the polish transition + active scale class', () => {
    render(<Button>Press me</Button>)
    const btn = screen.getByRole('button', { name: 'Press me' })
    expect(btn.className).toMatch(/active:scale-\[/)
    expect(btn.className).toMatch(/transition-\[/)
  })
})

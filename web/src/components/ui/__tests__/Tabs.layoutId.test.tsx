import { describe, it, expect } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { useState } from 'react'
import { Tabs } from '../Tabs'

function Harness() {
  const [active, setActive] = useState('a')
  return (
    <Tabs
      tabs={[
        { key: 'a', label: 'A' },
        { key: 'b', label: 'B' },
        { key: 'c', label: 'C' },
      ]}
      active={active}
      onChange={setActive}
    />
  )
}

describe('Tabs layoutId sliding bar', () => {
  it('renders an active indicator under the current tab', () => {
    render(<Harness />)
    const bars = document.querySelectorAll('[data-tab-active-bar]')
    expect(bars.length).toBe(1)
  })

  it('moves the active indicator when a different tab is clicked', () => {
    render(<Harness />)
    const tabB = screen.getByRole('tab', { name: 'B' })
    fireEvent.click(tabB)
    const bars = document.querySelectorAll('[data-tab-active-bar]')
    expect(bars.length).toBe(1)
    expect(tabB.getAttribute('aria-selected')).toBe('true')
  })
})

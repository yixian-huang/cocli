import { describe, it, expect } from 'vitest'
import { render } from '@testing-library/react'
import { MenuList, type MenuEntry } from '../MenuList'

describe('MenuList polish states', () => {
  it('selected item has accent left rail class', () => {
    const items: MenuEntry[] = [
      { id: 'a', label: 'A', onSelect: () => {} },
      { id: 'b', label: 'B', selected: true, onSelect: () => {} },
    ]
    const { container } = render(<MenuList items={items} />)
    const selectedItem = container.querySelector(
      '[data-menu-selected="true"]',
    ) as HTMLElement
    expect(selectedItem).not.toBeNull()
    expect(selectedItem.className).toMatch(/border-l-2/)
    expect(selectedItem.className).toMatch(/border-l-accent-signature/)
  })

  it('non-selected items have transparent left border (placeholder)', () => {
    const items: MenuEntry[] = [
      { id: 'a', label: 'A', onSelect: () => {} },
      { id: 'b', label: 'B', onSelect: () => {} },
    ]
    const { container } = render(<MenuList items={items} />)
    const els = container.querySelectorAll('[data-menu-item]')
    expect(els.length).toBe(2)
    els.forEach((el) => {
      expect(el.className).toMatch(/border-l-2/)
      expect(el.className).toMatch(/border-l-transparent/)
    })
  })
})

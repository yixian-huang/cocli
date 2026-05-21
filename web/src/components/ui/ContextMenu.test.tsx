import { afterEach, describe, expect, it } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { ContextMenuTrigger, ContextMenuPortal, type MenuEntry } from './ContextMenu'

const items: MenuEntry[] = [
  { id: 'rename', label: 'Rename', onSelect: () => {} },
  { id: 'delete', label: 'Delete', danger: true, onSelect: () => {} },
]

function Harness() {
  return (
    <>
      <ContextMenuPortal />
      <ContextMenuTrigger items={items}>
        <div data-testid="row" style={{ width: 100, height: 30 }}>row</div>
      </ContextMenuTrigger>
    </>
  )
}

describe('<ContextMenu>', () => {
  afterEach(() => {
    fireEvent.keyDown(document.body, { key: 'Escape' })
    cleanup()
  })

  it('opens on right-click and closes on Escape', () => {
    render(<Harness />)
    fireEvent.contextMenu(screen.getByTestId('row'), { clientX: 50, clientY: 50 })
    expect(screen.getByText('Rename')).toBeInTheDocument()
    fireEvent.keyDown(document.body, { key: 'Escape' })
    expect(screen.queryByText('Rename')).toBeNull()
  })

  it('only one menu is mounted at a time', () => {
    render(
      <>
        <ContextMenuPortal />
        <ContextMenuTrigger items={items}>
          <div data-testid="a" style={{ width: 50, height: 30 }}>a</div>
        </ContextMenuTrigger>
        <ContextMenuTrigger items={items}>
          <div data-testid="b" style={{ width: 50, height: 30 }}>b</div>
        </ContextMenuTrigger>
      </>
    )
    fireEvent.contextMenu(screen.getByTestId('a'), { clientX: 10, clientY: 10 })
    expect(screen.getAllByText('Rename')).toHaveLength(1)
    fireEvent.contextMenu(screen.getByTestId('b'), { clientX: 80, clientY: 10 })
    expect(screen.getAllByText('Rename')).toHaveLength(1)
  })

  it('keyboard nav: ArrowDown moves highlight, Enter selects highlighted item', () => {
    let picked = ''
    const items2: MenuEntry[] = [
      { id: 'rename', label: 'Rename', onSelect: () => (picked = 'rename') },
      { id: 'delete', label: 'Delete', onSelect: () => (picked = 'delete') },
    ]
    render(
      <>
        <ContextMenuPortal />
        <ContextMenuTrigger items={items2}>
          <div data-testid="row" style={{ width: 50, height: 30 }}>row</div>
        </ContextMenuTrigger>
      </>
    )
    fireEvent.contextMenu(screen.getByTestId('row'), { clientX: 10, clientY: 10 })
    fireEvent.keyDown(document.body, { key: 'ArrowDown' })
    fireEvent.keyDown(document.body, { key: 'Enter' })
    expect(picked).toBe('delete')
  })

  it('Enter on initial selection picks the first item', () => {
    let picked = ''
    const items3: MenuEntry[] = [
      { id: 'rename', label: 'Rename', onSelect: () => (picked = 'rename') },
      { id: 'delete', label: 'Delete', onSelect: () => (picked = 'delete') },
    ]
    render(
      <>
        <ContextMenuPortal />
        <ContextMenuTrigger items={items3}>
          <div data-testid="row" style={{ width: 50, height: 30 }}>row</div>
        </ContextMenuTrigger>
      </>
    )
    fireEvent.contextMenu(screen.getByTestId('row'), { clientX: 10, clientY: 10 })
    fireEvent.keyDown(document.body, { key: 'Enter' })
    expect(picked).toBe('rename')
  })
})

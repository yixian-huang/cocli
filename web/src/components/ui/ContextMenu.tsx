import { useEffect, useRef, useState } from 'react'
import { create } from 'zustand'
import { MenuList, type MenuEntry } from './MenuList'

interface OpenState {
  items: MenuEntry[]
  x: number
  y: number
}

interface CtxState {
  open: OpenState | null
  show: (s: OpenState) => void
  hide: () => void
}

const useCtxStore = create<CtxState>((set) => ({
  open: null,
  show: (s) => set({ open: s }),
  hide: () => set({ open: null }),
}))

interface TriggerProps {
  items: MenuEntry[]
  disabled?: boolean
  children: React.ReactElement
}

export function ContextMenuTrigger({ items, disabled, children }: TriggerProps) {
  const handler = (e: React.MouseEvent) => {
    if (disabled || items.length === 0) return
    e.preventDefault()
    useCtxStore.getState().show({ items, x: e.clientX, y: e.clientY })
  }
  return (
    <div onContextMenu={handler} style={{ display: 'contents' }}>
      {children}
    </div>
  )
}

function enabledItems(items: MenuEntry[]): Exclude<MenuEntry, '---'>[] {
  return items.filter((x): x is Exclude<MenuEntry, '---'> => x !== '---')
}

export function ContextMenuPortal() {
  const open = useCtxStore((s) => s.open)
  const hide = useCtxStore((s) => s.hide)
  const [activeIndex, setActiveIndex] = useState(0)
  const activeIndexRef = useRef(0)
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    activeIndexRef.current = activeIndex
  }, [activeIndex])

  useEffect(() => {
    if (open) setActiveIndex(0)
  }, [open])

  useEffect(() => {
    if (!open) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        hide()
      } else if (e.key === 'ArrowDown') {
        e.preventDefault()
        const max = enabledItems(open.items).length - 1
        setActiveIndex((i) => Math.min(i + 1, max))
      } else if (e.key === 'ArrowUp') {
        e.preventDefault()
        setActiveIndex((i) => Math.max(i - 1, 0))
      } else if (e.key === 'Enter') {
        e.preventDefault()
        const enabled = enabledItems(open.items)
        const item = enabled[activeIndexRef.current]
        if (item && !item.disabled) {
          item.onSelect()
          hide()
        }
      }
    }
    const onClick = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) hide()
    }
    document.addEventListener('keydown', onKey)
    document.addEventListener('mousedown', onClick)
    return () => {
      document.removeEventListener('keydown', onKey)
      document.removeEventListener('mousedown', onClick)
    }
  }, [open, hide])

  if (!open) return null

  const W = 200
  const H = open.items.length * 32 + 8
  const left = Math.min(open.x, window.innerWidth - W - 4)
  const top = Math.min(open.y, window.innerHeight - H - 4)

  return (
    <div ref={ref} style={{ position: 'fixed', left, top, zIndex: 80 }} role="menu">
      <MenuList items={open.items} onClose={hide} />
    </div>
  )
}

export type { MenuEntry } from './MenuList'

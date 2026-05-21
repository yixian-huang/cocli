import { useState, useRef, useEffect, createContext, useContext } from 'react'
import { AnimatePresence, motion } from 'framer-motion'
import { cn } from '@/lib/utils'
import { MenuList, type MenuEntry } from './MenuList'

const DropdownContext = createContext<{ close: () => void }>({ close: () => {} })

interface DropdownProps {
  trigger: React.ReactNode
  children?: React.ReactNode
  items?: MenuEntry[]
  align?: 'left' | 'right'
  className?: string
}

function Dropdown({ trigger, children, items, align = 'left', className }: DropdownProps) {
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const handleClick = (e: MouseEvent) => { if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false) }
    const handleEsc = (e: KeyboardEvent) => { if (e.key === 'Escape') setOpen(false) }
    document.addEventListener('mousedown', handleClick)
    document.addEventListener('keydown', handleEsc)
    return () => { document.removeEventListener('mousedown', handleClick); document.removeEventListener('keydown', handleEsc) }
  }, [open])

  return (
    <DropdownContext.Provider value={{ close: () => setOpen(false) }}>
      <div ref={ref} className="relative inline-block">
        <div onClick={() => setOpen(!open)}>{trigger}</div>
        <AnimatePresence>
          {open && (
            <motion.div
              initial={{ opacity: 0, scale: 0.92 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.92 }}
              transition={{ duration: 0.18, ease: [0.16, 1, 0.3, 1] }}
              className={cn(
                'absolute z-50 mt-1 min-w-[160px] rounded-[var(--radius-base)] border border-border bg-surface-panel p-1',
                'shadow-elev-hover',
                align === 'right' ? 'right-0' : 'left-0',
                className,
              )}
            >
              {items ? <MenuList items={items} onClose={() => setOpen(false)} /> : children}
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </DropdownContext.Provider>
  )
}

interface DropdownItemProps {
  onClick: () => void
  children: React.ReactNode
  danger?: boolean
  disabled?: boolean
}

function DropdownItem({ onClick, children, danger, disabled }: DropdownItemProps) {
  const { close } = useContext(DropdownContext)
  return (
    <button
      disabled={disabled}
      onClick={() => { onClick(); close() }}
      className={cn(
        'flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-sm transition-colors',
        'disabled:pointer-events-none disabled:opacity-50',
        danger ? 'text-destructive hover:bg-destructive/10' : 'text-popover-foreground hover:bg-accent'
      )}
    >
      {children}
    </button>
  )
}

Dropdown.Item = DropdownItem
export { Dropdown, DropdownItem }

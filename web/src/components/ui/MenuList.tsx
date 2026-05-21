import { cn } from '@/lib/utils'

export interface MenuItemBase {
  id: string
  label: string
  icon?: React.ReactNode
  shortcut?: string
  danger?: boolean
  disabled?: boolean
  selected?: boolean
  onSelect: () => void
}
export type MenuEntry = MenuItemBase | '---'

interface Props {
  items: MenuEntry[]
  onClose?: () => void
  className?: string
}

export function MenuList({ items, onClose, className }: Props) {
  return (
    <div
      className={cn(
        'min-w-[170px] rounded-md border border-border bg-popover p-1 shadow-md text-sm',
        className,
      )}
    >
      {items.map((entry, i) => {
        if (entry === '---') return <div key={`sep-${i}`} className="my-1 h-px bg-border" />
        return (
          <button
            key={entry.id}
            type="button"
            disabled={entry.disabled}
            data-menu-item
            data-menu-selected={entry.selected ? 'true' : undefined}
            onClick={() => {
              entry.onSelect()
              onClose?.()
            }}
            className={cn(
              'flex w-full items-center justify-between gap-3 rounded-sm px-3 py-1.5',
              'border-l-2',
              'transition-colors duration-[80ms]',
              'disabled:pointer-events-none disabled:opacity-50',
              entry.selected
                ? 'border-l-accent-signature bg-state-active-bg text-content-primary'
                : entry.danger
                  ? 'border-l-transparent text-destructive hover:bg-destructive/10'
                  : 'border-l-transparent text-popover-foreground hover:bg-state-hover-bg hover:text-content-primary',
              'active:bg-state-active-bg active:shadow-elev-press',
            )}
          >
            <span className="flex items-center gap-2">
              {entry.icon}
              {entry.label}
            </span>
            {entry.shortcut && (
              <span className="text-[11px] text-muted-foreground">{entry.shortcut}</span>
            )}
          </button>
        )
      })}
    </div>
  )
}

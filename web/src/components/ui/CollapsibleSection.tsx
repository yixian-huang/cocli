import { ChevronRight } from 'lucide-react'
import { cn } from '@/lib/utils'
import { useCollapsibleState } from '@/hooks/useCollapsibleState'

interface Props {
  id: string
  title: string
  count?: number
  actions?: React.ReactNode
  defaultCollapsed?: boolean
  children?: React.ReactNode
  className?: string
}

export function CollapsibleSection({
  id,
  title,
  count,
  actions,
  defaultCollapsed,
  children,
  className,
}: Props) {
  const [collapsed, setCollapsedState] = useCollapsibleState(id, defaultCollapsed)
  const onToggle = () => setCollapsedState(!collapsed)
  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault()
      onToggle()
    }
  }
  return (
    <section className={cn('border-b border-sidebar-border', className)}>
      <div className="flex items-center justify-between gap-2 pl-3 pr-2 py-2">
        <button
          type="button"
          onClick={onToggle}
          onKeyDown={onKeyDown}
          aria-expanded={!collapsed}
          className="flex flex-1 items-center gap-1 text-[11px] uppercase tracking-wide text-muted-foreground font-semibold focus:outline-none focus-visible:ring-1 focus-visible:ring-ring rounded"
        >
          <ChevronRight className={cn('h-3 w-3 transition-transform', !collapsed && 'rotate-90')} />
          <span>{title}</span>
          {typeof count === 'number' && (
            <span className="ml-1 text-muted-foreground/60 font-normal">{count}</span>
          )}
        </button>
        {actions && <div className="flex items-center">{actions}</div>}
      </div>
      {!collapsed && <div>{children}</div>}
    </section>
  )
}

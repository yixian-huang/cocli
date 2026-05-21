import { cn } from '@/lib/utils'

interface SectionHeaderProps {
  title: string
  action?: React.ReactNode
  className?: string
}

function SectionHeader({ title, action, className }: SectionHeaderProps) {
  return (
    <div className={cn('flex items-center justify-between px-2 mb-1', className)}>
      <span className="text-sm font-semibold uppercase tracking-wider text-content-secondary">{title}</span>
      {action}
    </div>
  )
}

export { SectionHeader }

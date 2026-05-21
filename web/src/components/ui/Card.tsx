import { cn } from '@/lib/utils'

interface CardProps {
  children: React.ReactNode
  header?: React.ReactNode
  footer?: React.ReactNode
  className?: string
  interactive?: boolean
}

function Card({ children, header, footer, className, interactive }: CardProps) {
  return (
    <div
      className={cn(
        'rounded-lg border border-border bg-card text-card-foreground shadow-sm',
        interactive && [
          'cursor-pointer',
          'transition-[background-color,box-shadow,transform,border-color]',
          'duration-[var(--motion-base)] ease-[var(--motion-fn)]',
          'shadow-elev-rest',
          'hover:shadow-elev-hover',
          'hover:translate-y-[var(--card-hover-lift)]',
          'active:scale-[0.99] active:shadow-elev-press active:duration-[var(--motion-fast)]',
        ],
        className,
      )}
    >
      {header && <div className="px-4 py-3 border-b border-border">{header}</div>}
      <div className="p-4">{children}</div>
      {footer && <div className="px-4 py-3 border-t border-border">{footer}</div>}
    </div>
  )
}

export { Card }

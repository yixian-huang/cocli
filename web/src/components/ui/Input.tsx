import { forwardRef } from 'react'
import { cn } from '@/lib/utils'

interface InputProps extends React.InputHTMLAttributes<HTMLInputElement> {
  label?: string
  error?: string
  iconLeft?: React.ReactNode
  iconRight?: React.ReactNode
}

const Input = forwardRef<HTMLInputElement, InputProps>(
  ({ className, label, error, iconLeft, iconRight, ...props }, ref) => (
    <div className="w-full">
      {label && <label className="block text-sm font-medium text-foreground mb-1.5">{label}</label>}
      <div className="relative">
        {iconLeft && <div className="absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground">{iconLeft}</div>}
        <input
          ref={ref}
          aria-invalid={error ? true : undefined}
          className={cn(
            'flex h-9 w-full rounded-md border bg-surface-panel px-3 py-2 text-base',
            'border-border-default',
            'transition-[border-color,box-shadow] duration-[var(--motion-base)] ease-[var(--motion-fn)]',
            'hover:border-border-strong',
            'focus-visible:outline-none focus-visible:shadow-[0_0_0_2px_var(--accent-signature)] focus-visible:border-transparent',
            'aria-[invalid=true]:border-[var(--feedback-error)]',
            'placeholder:text-content-muted',
            'disabled:cursor-not-allowed disabled:opacity-50 disabled:pointer-events-none',
            error && 'border-[var(--feedback-error)]',
            iconLeft && 'pl-8', iconRight && 'pr-8',
            className
          )}
          {...props}
        />
        {iconRight && <div className="absolute right-2.5 top-1/2 -translate-y-1/2 text-muted-foreground">{iconRight}</div>}
      </div>
      {error && <p className="mt-1.5 text-sm text-destructive">{error}</p>}
    </div>
  )
)
Input.displayName = 'Input'
export { Input }

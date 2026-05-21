import { forwardRef } from 'react'
import { cva, type VariantProps } from 'class-variance-authority'
import { cn } from '@/lib/utils'
import { Loader2 } from 'lucide-react'

const buttonVariants = cva(
  [
    'inline-flex items-center justify-center gap-2 rounded-md font-medium',
    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
    'disabled:pointer-events-none disabled:opacity-50',
    'transition-[background-color,box-shadow,transform,border-color]',
    'duration-[var(--motion-base)] ease-[var(--motion-fn)]',
    'active:scale-[var(--state-press-scale)] active:shadow-elev-press active:duration-[var(--motion-fast)]',
  ].join(' '),
  {
    variants: {
      variant: {
        primary: 'bg-primary text-primary-foreground hover:bg-primary-hover hover:shadow-elev-hover',
        secondary: 'bg-secondary text-secondary-foreground hover:bg-secondary-hover hover:shadow-elev-hover border border-border',
        ghost: 'hover:bg-state-hover-bg hover:text-accent-foreground',
        danger: 'bg-destructive text-destructive-foreground hover:bg-destructive/90 hover:shadow-elev-hover',
      },
      size: {
        xs: 'text-xs px-2 py-1 h-7',
        sm: 'text-sm px-2.5 py-1 h-8',
        md: 'text-base px-3.5 py-2 h-9',
        lg: 'text-base px-4 py-2.5 h-11',
      },
    },
    defaultVariants: { variant: 'primary', size: 'md' },
  }
)

interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement>, VariantProps<typeof buttonVariants> {
  loading?: boolean
}

const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, loading, children, disabled, ...props }, ref) => (
    <button ref={ref} className={cn(buttonVariants({ variant, size }), className)} disabled={disabled || loading} {...props}>
      {loading && <Loader2 className="h-4 w-4 animate-spin" />}
      {loading ? null : children}
    </button>
  )
)
Button.displayName = 'Button'
export { Button, buttonVariants }

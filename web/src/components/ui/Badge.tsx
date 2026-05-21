import { cva, type VariantProps } from 'class-variance-authority'
import { cn } from '@/lib/utils'

const badgeVariants = cva('inline-flex items-center rounded-md font-medium', {
  variants: {
    variant: {
      default: 'bg-secondary text-secondary-foreground',
      success: 'bg-success/10 text-success',
      warning: 'bg-warning/10 text-warning',
      error: 'bg-error/10 text-error',
      info: 'bg-info/10 text-info',
    },
    size: {
      sm: 'text-xs px-2 py-0.5',
      md: 'text-sm px-2.5 py-0.5',
    },
  },
  defaultVariants: { variant: 'default', size: 'md' },
})

interface BadgeProps extends VariantProps<typeof badgeVariants>, React.HTMLAttributes<HTMLSpanElement> {
  children: React.ReactNode
  className?: string
}

function Badge({ variant, size, className, children, ...props }: BadgeProps) {
  return <span className={cn(badgeVariants({ variant, size }), className)} {...props}>{children}</span>
}

export { Badge, badgeVariants }

import { cn } from '@/lib/utils'

interface SkeletonProps {
  variant?: 'line' | 'circle' | 'rectangle'
  width?: string | number
  height?: string | number
  className?: string
}

function Skeleton({ variant = 'line', width, height, className }: SkeletonProps) {
  const variantClasses = { line: 'h-3 w-full rounded', circle: 'rounded-full', rectangle: 'rounded-md' }

  return (
    <div
      className={cn('bg-muted', variantClasses[variant], className)}
      style={{
        width: width ?? (variant === 'circle' ? 32 : undefined),
        height: height ?? (variant === 'circle' ? 32 : variant === 'rectangle' ? 80 : undefined),
        backgroundImage: 'linear-gradient(90deg, transparent 0%, var(--muted-foreground) 50%, transparent 100%)',
        backgroundSize: '200% 100%',
        animation: 'shimmer 1.5s ease-in-out infinite',
        opacity: 0.1,
      }}
    />
  )
}

export { Skeleton }

import { cn } from '@/lib/utils'

interface StatusDotProps {
  status: 'online' | 'offline' | 'working' | 'error'
  size?: 'sm' | 'md'
  variant?: 'classic' | 'signature'
  className?: string
}

const statusColors: Record<StatusDotProps['status'], string> = {
  online:  'var(--feedback-success)',
  working: 'var(--accent-signature)',
  error:   'var(--feedback-error)',
  offline: 'var(--content-subtle)',
}

const sizes = { sm: 'w-1.5 h-1.5', md: 'w-2 h-2' }

function StatusDot({ status, size = 'md', variant = 'classic', className }: StatusDotProps) {
  const isSignature = variant === 'signature'
  return (
    <span
      className={cn(
        'inline-block shrink-0',
        isSignature ? 'rounded-[1px]' : 'rounded-full',
        sizes[size],
        status === 'working' && (isSignature ? 'animate-signature-breathe' : 'animate-pulse'),
        className,
      )}
      style={{
        backgroundColor: statusColors[status],
        boxShadow: isSignature && status !== 'offline' ? 'var(--shadow-signature-glow)' : undefined,
      }}
    />
  )
}

export { StatusDot }

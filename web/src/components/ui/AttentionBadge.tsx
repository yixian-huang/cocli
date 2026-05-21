import { Badge } from './Badge'
import { agentAttentionLabel, agentAttentionTitle, agentAttentionTone } from '@/lib/status'
import type { AgentAttentionState } from '@/lib/types'
import { cn } from '@/lib/utils'
import { AlertTriangle, Clock, Gauge, Loader2, RefreshCw, SlidersHorizontal, Zap } from 'lucide-react'

const toneClasses = {
  success: 'bg-success/10 text-success ring-success/20',
  info: 'bg-info/10 text-info ring-info/20',
  warn: 'bg-warning/10 text-warning ring-warning/20',
  danger: 'bg-error/10 text-error ring-error/20',
  neutral: 'bg-secondary text-secondary-foreground ring-border/60',
} as const

type AttentionBadgeProps = {
  state: AgentAttentionState
  className?: string
  iconPosition?: 'left' | 'right'
  showIcon?: boolean
}

function AttentionIcon({ state }: { state: AgentAttentionState }) {
  const className = 'h-3 w-3 shrink-0'

  switch (state) {
    case 'working':
      return <Loader2 aria-hidden="true" className={cn(className, 'animate-spin')} />
    case 'focus':
      return <Zap aria-hidden="true" className={className} />
    case 'preempting':
      return <RefreshCw aria-hidden="true" className={cn(className, 'animate-spin')} />
    case 'stalled':
    case 'context_overflow':
      return <AlertTriangle aria-hidden="true" className={className} />
    case 'context_pressure':
      return <Gauge aria-hidden="true" className={className} />
    case 'backstop_threshold_adjusted':
      return <SlidersHorizontal aria-hidden="true" className={className} />
    case 'rate_limited':
      return <Clock aria-hidden="true" className={className} />
    default:
      return null
  }
}

export function AttentionBadge({
  state,
  className,
  iconPosition = 'left',
  showIcon = true,
}: AttentionBadgeProps) {
  const tone = agentAttentionTone(state)

  return (
    <Badge
      variant="default"
      size="sm"
      title={agentAttentionTitle(state)}
      className={cn(
        'gap-1 rounded-full px-2 py-0.5 text-[10px] leading-4 ring-1 ring-inset',
        toneClasses[tone],
        iconPosition === 'right' && 'flex-row-reverse',
        className,
      )}
    >
      {showIcon ? <AttentionIcon state={state} /> : null}
      <span>{agentAttentionLabel(state)}</span>
    </Badge>
  )
}

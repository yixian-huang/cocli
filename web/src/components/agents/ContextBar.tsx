import { cn } from '@/lib/utils'

interface ContextBarProps {
  lastInputTokens?: number
  contextWindow?: number
  totalOutputTokens?: number
  totalCostUSD?: number
  turnCount?: number
  variant?: 'full' | 'inline'
  className?: string
}

const L1 = 0.20
const L2 = 0.50
const L3 = 0.80

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`
  return String(n)
}

function colorFor(ratio: number): string {
  return ratio >= L2 ? 'bg-amber-500' : ratio >= L1 ? 'bg-blue-500' : 'bg-emerald-500'
}

function labelFor(ratio: number): string {
  return ratio >= L2 ? 'High' : ratio >= L1 ? 'Moderate' : 'Normal'
}

export function ContextBar({
  lastInputTokens,
  contextWindow,
  totalOutputTokens,
  totalCostUSD,
  turnCount,
  variant = 'full',
  className,
}: ContextBarProps) {
  if (!contextWindow || !lastInputTokens) return null

  const ratio = lastInputTokens / contextWindow
  const pct = Math.min(ratio * 100, 100)
  const barColor = colorFor(ratio)

  if (variant === 'inline') {
    return (
      <div
        data-testid="context-bar-inline"
        className={cn('flex items-center gap-2 text-[11px] text-muted-foreground', className)}
      >
        <div className="relative h-1.5 w-20 rounded-full bg-muted overflow-hidden">
          <div className={cn('h-full rounded-full transition-all duration-500', barColor)} style={{ width: `${pct}%` }} />
        </div>
        <span>{Math.round(pct)}%</span>
        {turnCount != null && turnCount > 0 && <span>· {turnCount} turns</span>}
        {totalCostUSD != null && totalCostUSD > 0 && <span>· ${totalCostUSD.toFixed(2)}</span>}
      </div>
    )
  }

  const label = labelFor(ratio)
  return (
    <div className={cn('space-y-1.5', className)}>
      <div className="relative h-2 rounded-full bg-muted overflow-hidden">
        <div className={cn('h-full rounded-full transition-all duration-500', barColor)} style={{ width: `${pct}%` }} />
        <div className="absolute inset-0 pointer-events-none">
          <div className="absolute top-0 h-full w-px bg-foreground/20" style={{ left: `${L1 * 100}%` }} title="L1 (20%)" />
          <div className="absolute top-0 h-full w-px bg-foreground/30" style={{ left: `${L2 * 100}%` }} title="L2 (50%)" />
          <div className="absolute top-0 h-full w-px bg-foreground/40" style={{ left: `${L3 * 100}%` }} title="L3 (80%)" />
        </div>
      </div>
      <div className="flex items-center justify-between text-[10px] text-muted-foreground">
        <span>
          <span className={cn('inline-block w-1.5 h-1.5 rounded-full mr-1', barColor)} />
          {Math.round(pct)}% context &middot; {label}
        </span>
        <span>{formatTokens(lastInputTokens)} / {formatTokens(contextWindow)}</span>
      </div>
      <div className="flex items-center gap-3 text-[10px] text-muted-foreground/70">
        {turnCount != null && turnCount > 0 && <span>{turnCount} turns</span>}
        {totalOutputTokens != null && totalOutputTokens > 0 && <span>{formatTokens(totalOutputTokens)} output</span>}
        {totalCostUSD != null && totalCostUSD > 0 && <span>${totalCostUSD.toFixed(3)}</span>}
      </div>
    </div>
  )
}

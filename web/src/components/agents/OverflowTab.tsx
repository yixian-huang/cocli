import { useEffect, useMemo, useState } from 'react'
import { overflowStats as overflowStatsApi } from '@/api/client'
import type { Agent, OverflowStatsEntry } from '@/lib/types'

function formatPct(value: number): string {
  return `${Math.round(value * 100)}%`
}

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`
  return String(n)
}

function formatSessionAge(seconds: number): string {
  if (seconds >= 3600) return `${Math.round(seconds / 3600)}h`
  if (seconds >= 60) return `${Math.round(seconds / 60)}m`
  return `${seconds}s`
}

export function OverflowTab({ agent }: { agent: Agent }) {
  const [entries, setEntries] = useState<OverflowStatsEntry[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    setLoading(true)
    setError(null)

    overflowStatsApi.list()
      .then((next) => {
        if (cancelled) return
        setEntries(next)
        setLoading(false)
      })
      .catch((err) => {
        if (cancelled) return
        setError(err instanceof Error ? err.message : 'Failed to load overflow stats')
        setLoading(false)
      })

    return () => {
      cancelled = true
    }
  }, [])

  const entry = useMemo(
    () => entries.find((item) => item.driver === agent.runtime && item.model === agent.model) ?? null,
    [agent.model, agent.runtime, entries],
  )

  if (loading) {
    return <div className="flex-1 overflow-y-auto p-4 text-sm text-muted-foreground">Loading overflow telemetry…</div>
  }

  if (error) {
    return <div className="flex-1 overflow-y-auto p-4 text-sm text-red-600 dark:text-red-400">{error}</div>
  }

  if (!entry) {
    return (
      <div className="flex-1 overflow-y-auto p-4 space-y-3">
        <p className="text-sm text-muted-foreground">
          No overflow bucket is registered for <span className="font-mono">{agent.runtime}/{agent.model}</span> yet.
        </p>
      </div>
    )
  }

  const deltaPct = Math.round((entry.currentBackstopPct - entry.defaultBackstopPct) * 100)
  const progressPct = Math.max(0, Math.min(100, (entry.forksSinceLastOverflow / 20) * 100))

  return (
    <div className="flex-1 overflow-y-auto p-4 space-y-6">
      <section className="space-y-2">
        <h4 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">Overflow</h4>
        <p className="text-sm text-muted-foreground">
          Backstop thresholds adapt from real overflow signals. `CTX_CRIT_PCT` still overrides runtime learning when set.
        </p>
      </section>

      <section className="grid gap-3 md:grid-cols-2">
        <MetricCard
          label="Current Backstop"
          value={formatPct(entry.currentBackstopPct)}
          detail={deltaPct === 0 ? `Default ${formatPct(entry.defaultBackstopPct)}` : `${deltaPct > 0 ? '+' : ''}${deltaPct}pp vs default ${formatPct(entry.defaultBackstopPct)}`}
        />
        <MetricCard
          label="Overflow Count"
          value={String(entry.overflowCount)}
          detail={`Context window ${formatTokens(entry.contextWindowTokens)}`}
        />
        <MetricCard
          label="Fork Progress"
          value={`${entry.forksSinceLastOverflow}/20`}
          detail="Successful forks since last overflow"
        />
        <MetricCard
          label="Last Adjusted"
          value={entry.lastAdjustedAt ? new Date(entry.lastAdjustedAt).toLocaleString() : 'Default only'}
          detail={`${agent.runtime}/${agent.model}`}
        />
      </section>

      <section className="space-y-2">
        <div className="flex items-center justify-between text-xs text-muted-foreground">
          <span>Forks Since Last Overflow</span>
          <span>{entry.forksSinceLastOverflow}/20</span>
        </div>
        <div className="h-2 rounded-full bg-secondary">
          <div
            className="h-2 rounded-full bg-primary transition-[width]"
            style={{ width: `${progressPct}%` }}
          />
        </div>
      </section>

      <section className="space-y-3">
        <h4 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">Recent Overflows</h4>
        {entry.recentOverflows.length === 0 ? (
          <p className="text-sm text-muted-foreground">No overflow events recorded for this runtime/model yet.</p>
        ) : (
          <div className="space-y-2">
            {entry.recentOverflows.map((item, index) => (
              <div key={`${item.occurredAt}-${index}`} className="rounded-lg border border-border bg-card px-3 py-2">
                <div className="flex items-center justify-between gap-3 text-sm">
                  <span className="font-medium">Overflow at {formatPct(item.utilPct)}</span>
                  <span className="text-xs text-muted-foreground">{new Date(item.occurredAt).toLocaleString()}</span>
                </div>
                <div className="mt-1 flex items-center gap-3 text-xs text-muted-foreground">
                  <span>Session age {formatSessionAge(item.sessionAgeSeconds)}</span>
                  {item.contextWindowTokens ? <span>Window {formatTokens(item.contextWindowTokens)}</span> : null}
                </div>
              </div>
            ))}
          </div>
        )}
      </section>
    </div>
  )
}

function MetricCard({ label, value, detail }: { label: string; value: string; detail: string }) {
  return (
    <div className="rounded-lg border border-border bg-card px-3 py-3">
      <div className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">{label}</div>
      <div className="mt-2 text-lg font-semibold text-foreground">{value}</div>
      <div className="mt-1 text-xs text-muted-foreground">{detail}</div>
    </div>
  )
}

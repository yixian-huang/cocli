import { useAgentStore } from '@/stores/agentStore'
import type { TrajectoryEntry } from '@/lib/types'
import { useViewStore } from '@/stores/viewStore'
import { ContextBar } from '../ContextBar'
import { StatusDot } from '@/components/ui'

const MAX_RECENT = 20
const EMPTY_ENTRIES: TrajectoryEntry[] = []

function formatTs(ts?: number): string {
  if (!ts) return ''
  return new Date(ts).toLocaleTimeString()
}

export function LivePanel({ agentId }: { agentId: string }) {
  const agent = useAgentStore((s) => s.agents.find((a) => a.id === agentId))
  const entries = useAgentStore((s) => s.currentTurnEntries[agentId] ?? EMPTY_ENTRIES)
  const openHistoryAt = useViewStore((s) => s.openHistoryAt)

  const hasCtx = !!(agent?.contextWindow && agent.contextWindow > 0)
  const hasEntries = entries.length > 0

  if (!agent || (!hasCtx && !hasEntries)) {
    return (
      <div data-testid="drawer-live" className="p-4 text-sm text-muted-foreground">
        No live data — agent is offline or has not started a session.
      </div>
    )
  }

  const recent = entries.slice(-MAX_RECENT).reverse()

  return (
    <div data-testid="drawer-live" className="flex flex-col h-full">
      <section className="p-3 border-b">
        <div className="flex items-center gap-2 mb-2 text-xs">
          <StatusDot status={agent.status as 'online' | 'offline' | 'working' | 'error'} />
          <span className="font-semibold">@{agent.name}</span>
          <span className="text-muted-foreground">{agent.status}</span>
        </div>
        {hasCtx && (
          <ContextBar
            lastInputTokens={agent.lastInputTokens}
            contextWindow={agent.contextWindow}
            totalOutputTokens={agent.totalOutputTokens}
            totalCostUSD={agent.totalCostUSD}
            turnCount={agent.turnCount}
            variant="full"
          />
        )}
      </section>

      <section className="flex-1 overflow-y-auto p-3 space-y-1.5">
        <div className="text-[10px] uppercase tracking-wider text-muted-foreground mb-1">
          Recent activity
        </div>
        {!hasEntries && (
          <div className="text-xs text-muted-foreground italic">No activity yet.</div>
        )}
        {recent.map((e, idx) => (
          <div key={e.id ?? idx} className="text-xs border-l-2 border-muted pl-2">
            <div className="flex items-center gap-2 text-muted-foreground text-[10px]">
              <span>{formatTs(e.ts)}</span>
              <span className="font-mono">{e.kind}</span>
            </div>
            {e.text && <div className="truncate">{e.text}</div>}
          </div>
        ))}
      </section>

      <footer className="border-t p-2">
        <button
          type="button"
          onClick={() => openHistoryAt('activity')}
          className="w-full text-xs text-primary hover:underline py-1"
        >
          View full activity →
        </button>
      </footer>
    </div>
  )
}

import { useState } from 'react'
import type { Turn, TrajectoryEntry } from '@/lib/types'
import { cn } from '@/lib/utils'
import { Badge } from '@/components/ui'
import { ChevronDown, ChevronRight, Loader2 } from 'lucide-react'
import { renderEntry } from './TurnEntryRenderers'

export function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`
  return String(n)
}

export function formatDuration(startedAt: string, endedAt?: string): string {
  const startMs = new Date(startedAt).getTime()
  const endMs = endedAt ? new Date(endedAt).getTime() : Date.now()
  const diff = Math.floor((endMs - startMs) / 1000)
  if (diff < 60) return `${diff}s`
  if (diff < 3600) return `${Math.floor(diff / 60)}m ${diff % 60}s`
  const h = Math.floor(diff / 3600)
  const m = Math.floor((diff % 3600) / 60)
  return `${h}h ${m}m`
}

function timeAgo(ts: string | number): string {
  const ms = typeof ts === 'number' ? ts * 1000 : new Date(ts).getTime()
  const diff = Math.floor((Date.now() - ms) / 1000)
  if (diff < 60) return `${diff}s ago`
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`
  return `${Math.floor(diff / 86400)}d ago`
}

function getToolNames(entries: TrajectoryEntry[]): string[] {
  const names: string[] = []
  for (const e of entries) {
    if (e.kind === 'tool_call' && e.input && typeof (e.input as { name?: string }).name === 'string') {
      const toolName = (e.input as { name?: string }).name!
      if (!names.includes(toolName)) names.push(toolName)
    }
  }
  return names
}

export function TurnCard({
  turn,
  defaultOpen,
  isLive,
}: {
  turn: Turn
  defaultOpen: boolean
  isLive?: boolean
}) {
  const [open, setOpen] = useState(defaultOpen)
  const entries = turn.entries ?? []
  const tools = getToolNames(entries)
  const tokens = (turn.inputTokens ?? 0) + (turn.outputTokens ?? 0)
  const ts = turn.startedAt

  return (
    <div
      className={cn(
        'rounded border mb-2 overflow-hidden transition-colors',
        isLive
          ? 'border-dashed border-success/35 bg-success/5'
          : 'border-border-default bg-surface-primary',
      )}
    >
      {/* Card header */}
      <button
        onClick={() => setOpen((v) => !v)}
        className="w-full flex items-center gap-2 px-3 py-2 text-left hover:bg-accent/30 transition-colors"
      >
        {open ? (
          <ChevronDown className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
        ) : (
          <ChevronRight className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
        )}
        <span className="text-[11px] font-medium text-foreground">
          Turn #{turn.turnNumber}
        </span>
        <span className="text-[10px] text-muted-foreground">
          {new Date(turn.startedAt).toLocaleTimeString()}
        </span>
        {turn.channelName && (
          <span className="rounded bg-surface-tertiary px-1.5 py-0.5 text-[10px] text-content-secondary">
            #{turn.channelName}
          </span>
        )}
        {tools.length > 0 && (
          <span className="text-[10px] text-muted-foreground font-mono truncate max-w-[160px]">
            {tools.join(', ')}
          </span>
        )}
        <span className="ml-auto flex items-center gap-2 shrink-0">
          {(turn.inputTokens || turn.outputTokens) ? (
            <span className="text-xs text-content-muted">
              {turn.inputTokens?.toLocaleString()} in / {turn.outputTokens?.toLocaleString()} out
              {turn.costUsd != null && turn.costUsd > 0 && ` · $${turn.costUsd.toFixed(3)}`}
            </span>
          ) : (
            <>
              {tokens > 0 && <Badge size="sm">{formatTokens(tokens)}t</Badge>}
              {turn.costUsd != null && turn.costUsd > 0 && (
                <Badge size="sm">${turn.costUsd.toFixed(4)}</Badge>
              )}
            </>
          )}
          {turn.contextUsagePct != null && turn.contextUsagePct > 80 && (
            <span className="rounded bg-warning/15 px-1.5 py-0.5 text-[10px] text-warning-emphasis">
              Context {Math.round(turn.contextUsagePct)}%
            </span>
          )}
          <span className="text-[10px] text-muted-foreground">
            {turn.endedAt ? formatDuration(turn.startedAt, turn.endedAt) : timeAgo(ts)}
          </span>
          {isLive && <Loader2 className="h-3 w-3 animate-spin text-success-emphasis" />}
        </span>
      </button>

      {/* Expanded entries */}
      {open && (
        <div className="px-4 pb-2 border-t divide-y divide-border/50">
          {entries.map((entry, i) => (
            <div key={i}>{renderEntry(entry)}</div>
          ))}
          {entries.length === 0 && (
            <p className="py-2 text-[11px] text-muted-foreground">No entries yet</p>
          )}
        </div>
      )}
    </div>
  )
}

export function FlowTurnDivider({ turn }: { turn: Turn }) {
  const tokens = (turn.inputTokens ?? 0) + (turn.outputTokens ?? 0)
  return (
    <div className="flex items-center gap-2 my-3">
      <div className="flex-1 h-px bg-border" />
      <span className="text-[10px] text-muted-foreground whitespace-nowrap flex items-center gap-2">
        <span className="font-medium">Turn #{turn.turnNumber}</span>
        <span>{new Date(turn.startedAt).toLocaleTimeString()}</span>
        {turn.channelName && (
          <span className="rounded bg-surface-tertiary px-1.5 py-0.5 text-[10px] text-content-secondary">
            #{turn.channelName}
          </span>
        )}
        {(turn.inputTokens || turn.outputTokens) ? (
          <span className="text-xs text-content-muted">
            {turn.inputTokens?.toLocaleString()} in / {turn.outputTokens?.toLocaleString()} out
            {turn.costUsd != null && turn.costUsd > 0 && ` · $${turn.costUsd.toFixed(3)}`}
          </span>
        ) : (
          <>
            {tokens > 0 && <Badge size="sm">{formatTokens(tokens)}t</Badge>}
            {turn.costUsd != null && turn.costUsd > 0 && (
              <Badge size="sm">${turn.costUsd.toFixed(4)}</Badge>
            )}
          </>
        )}
        {turn.contextUsagePct != null && turn.contextUsagePct > 80 && (
          <span className="rounded bg-warning/15 px-1.5 py-0.5 text-[10px] text-warning-emphasis">
            Context {Math.round(turn.contextUsagePct)}%
          </span>
        )}
        {turn.endedAt && <span>{formatDuration(turn.startedAt, turn.endedAt)}</span>}
      </span>
      <div className="flex-1 h-px bg-border" />
    </div>
  )
}

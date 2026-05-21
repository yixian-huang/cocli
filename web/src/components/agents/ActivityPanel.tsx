import { useState, useEffect, useRef } from 'react'
import { useAgentStore } from '@/stores/agentStore'
import { usePrefsStore } from '@/stores/prefsStore'
import { agentTurns, agentSessions } from '@/api/client'
import type { Turn, TrajectoryEntry, AgentSession } from '@/lib/types'
import { Button } from '@/components/ui'
import { Skeleton, TurnLogSkeleton } from '@/components/Skeleton'
import { Loader2, LayoutList, ScrollText, ChevronsDown, ChevronsUp, ArrowDownToLine } from 'lucide-react'
import { TurnCard, FlowTurnDivider } from './TurnCard'
import { renderEntry } from './TurnEntryRenderers'

type Forced = 'all' | 'none' | null

const EMPTY_TURNS: Turn[] = []
const EMPTY_ENTRIES: TrajectoryEntry[] = []

type ViewMode = 'timeline' | 'flow'

// ──────────────────────────────────────────────────────────────
// Timeline view
// ──────────────────────────────────────────────────────────────

function TimelineView({
  turns,
  liveTurn,
  defaultExpandLastN,
  forced,
  forcedSeq,
  bottomRef,
}: {
  turns: Turn[]
  liveTurn: Turn | null
  defaultExpandLastN: number
  forced: Forced
  forcedSeq: number
  bottomRef: React.RefObject<HTMLDivElement | null>
}) {
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [turns.length, liveTurn?.entries?.length, bottomRef])

  const cutoff = Math.max(0, turns.length - defaultExpandLastN)
  return (
    <div className="flex-1 min-h-0 overflow-y-auto p-3">
      {turns.map((turn, idx) => {
        const open =
          forced === 'all' ? true : forced === 'none' ? false : !liveTurn && idx >= cutoff
        return (
          <TurnCard
            key={`${turn.id}:${forcedSeq}`}
            turn={turn}
            defaultOpen={open}
          />
        )
      })}
      {liveTurn && (
        <TurnCard key={`__live__:${forcedSeq}`} turn={liveTurn} defaultOpen={true} isLive={true} />
      )}
      <div ref={bottomRef} />
    </div>
  )
}

// ──────────────────────────────────────────────────────────────
// Flow view
// ──────────────────────────────────────────────────────────────

function FlowView({
  turns,
  liveTurn,
}: {
  turns: Turn[]
  liveTurn: Turn | null
}) {
  const allTurns = liveTurn ? [...turns, liveTurn] : turns
  const bottomRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [allTurns.length, liveTurn?.entries?.length])

  return (
    <div className="flex-1 min-h-0 overflow-y-auto p-3">
      {allTurns.map((turn, idx) => (
        <div key={turn.id}>
          {idx > 0 && <FlowTurnDivider turn={turn} />}
          {idx === 0 && (
            <div className="flex items-center gap-2 mb-2">
              <div className="flex-1 h-px bg-border" />
              <span className="text-[10px] text-muted-foreground">
                Turn #{turn.turnNumber} · {new Date(turn.startedAt).toLocaleTimeString()}
              </span>
              <div className="flex-1 h-px bg-border" />
            </div>
          )}
          <div className="space-y-0.5">
            {(turn.entries ?? []).map((entry, i) => (
              <div key={i}>{renderEntry(entry)}</div>
            ))}
          </div>
          {turn.id === '__live__' && (
            <div className="mt-1 flex items-center gap-1 text-[11px] text-success-emphasis">
              <Loader2 className="h-3 w-3 animate-spin" />
              <span>Processing...</span>
            </div>
          )}
        </div>
      ))}
      <div ref={bottomRef} />
    </div>
  )
}

// ──────────────────────────────────────────────────────────────
// Main coordinator
// ──────────────────────────────────────────────────────────────

export function ActivityPanel({ agentId, loading: loadingProp }: { agentId: string; loading?: boolean }) {
  const [viewMode, setViewMode] = useState<ViewMode>(() => {
    try {
      return (localStorage.getItem('activityViewMode') as ViewMode) || 'timeline'
    } catch {
      return 'timeline'
    }
  })

  const [sessions, setSessions] = useState<AgentSession[]>([])
  const [selectedSession, setSelectedSession] = useState<string>('')
  const [loadingTurnsState, setLoadingTurns] = useState(true)
  const [forced, setForced] = useState<Forced>(null)
  const [forcedSeq, setForcedSeq] = useState(0)
  const bottomRef = useRef<HTMLDivElement>(null)
  const defaultExpandLastN =
    usePrefsStore((s) => s.prefs.ui?.activity?.defaultExpandLastN) ?? 3

  const applyForced = (next: Forced) => {
    setForced(next)
    setForcedSeq((n) => n + 1)
  }

  const goToLatest = () => {
    setForced(null)
    setForcedSeq((n) => n + 1)
    requestAnimationFrame(() => {
      bottomRef.current?.scrollIntoView({ behavior: 'smooth' })
    })
  }

  // From store: completed turns + live in-progress entries
  const storedTurns = useAgentStore((s) => s.turns[agentId] ?? EMPTY_TURNS)
  const currentEntries = useAgentStore((s) => s.currentTurnEntries[agentId] ?? EMPTY_ENTRIES)
  const setTurns = useAgentStore((s) => s.setTurns)
  const selectedSessionRef = useRef(selectedSession)
  selectedSessionRef.current = selectedSession

  // Load sessions on mount
  useEffect(() => {
    let cancelled = false
    agentSessions.list(agentId, 20).then((data) => {
      if (!cancelled && data) {
        setSessions(data)
      }
    }).catch((err) => console.warn('[api] sessions fetch failed:', err))
    return () => { cancelled = true }
  }, [agentId])

  // Load turns when agent or session changes
  useEffect(() => {
    let cancelled = false
    setLoadingTurns(true)
    agentTurns.list(agentId, selectedSession || undefined)
      .then((data) => { if (!cancelled) setTurns(agentId, data || []) })
      .catch(() => { if (!cancelled) setTurns(agentId, []) })
      .finally(() => { if (!cancelled) setLoadingTurns(false) })
    return () => { cancelled = true }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [agentId, selectedSession])

  const handleViewMode = (mode: ViewMode) => {
    setViewMode(mode)
    try {
      localStorage.setItem('activityViewMode', mode)
    } catch { /* ignore */ }
  }

  // Build live turn from currentEntries if any
  const liveTurn: Turn | null =
    currentEntries.length > 0
      ? {
          id: '__live__',
          agentId,
          sessionId: '',
          turnNumber: (storedTurns.length > 0 ? storedTurns[storedTurns.length - 1].turnNumber : 0) + 1,
          startedAt: new Date().toISOString(),
          entries: currentEntries,
        }
      : null

  // All completed turns for display — sorted by time, oldest first (top to bottom)
  const displayTurns = [...storedTurns].sort((a, b) =>
    new Date(a.startedAt).getTime() - new Date(b.startedAt).getTime()
  )
  const loadingTurns = loadingProp ?? loadingTurnsState

  return (
    <div className="flex-1 min-h-0 flex flex-col">
      {/* Top bar */}
      <div className="shrink-0 flex items-center gap-2 border-b border-border-default bg-surface-primary px-4 py-2">
        {/* View mode toggle */}
        <div className="flex items-center rounded border overflow-hidden">
          <Button
            variant={viewMode === 'timeline' ? 'primary' : 'ghost'}
            size="sm"
            onClick={() => handleViewMode('timeline')}
            title="Timeline view"
            className="rounded-none gap-1"
          >
            <LayoutList className="h-3 w-3" />
            Timeline
          </Button>
          <Button
            variant={viewMode === 'flow' ? 'primary' : 'ghost'}
            size="sm"
            onClick={() => handleViewMode('flow')}
            title="Flow view"
            className="rounded-none gap-1"
          >
            <ScrollText className="h-3 w-3" />
            Flow
          </Button>
        </div>

        {/* Bulk toolbar (timeline only — flow view shows everything inline) */}
        {viewMode === 'timeline' && (
          <div className="flex items-center gap-1 ml-2">
            <Button
              variant="ghost"
              size="sm"
              onClick={() => applyForced('all')}
              title="Expand all turns"
              aria-label="Expand all"
              className="gap-1"
            >
              <ChevronsDown className="h-3 w-3" />
              <span className="hidden sm:inline">Expand all</span>
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => applyForced('none')}
              title="Collapse all turns"
              aria-label="Collapse all"
              className="gap-1"
            >
              <ChevronsUp className="h-3 w-3" />
              <span className="hidden sm:inline">Collapse all</span>
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onClick={goToLatest}
              title="Jump to latest"
              aria-label="Latest"
              className="gap-1"
            >
              <ArrowDownToLine className="h-3 w-3" />
              <span className="hidden sm:inline">Latest</span>
            </Button>
          </div>
        )}

        {/* Session selector */}
        {loadingTurns ? (
          <Skeleton className="ml-auto h-7 w-[180px]" data-testid="turn-log-session-skeleton" />
        ) : sessions.length > 0 && (
          <select
            value={selectedSession}
            onChange={(e) => setSelectedSession(e.target.value)}
            className="ml-auto text-[11px] border rounded px-2 py-1 bg-background text-foreground max-w-[200px] truncate"
          >
            <option value="">All sessions</option>
            {sessions.map((s) => (
              <option key={s.id} value={s.sessionId}>
                {new Date(s.startedAt).toLocaleString()} · {s.turnCount}t
              </option>
            ))}
          </select>
        )}
      </div>

      {/* Content */}
      {loadingTurns ? (
        <TurnLogSkeleton viewMode={viewMode} />
      ) : displayTurns.length === 0 && !liveTurn ? (
        <div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
          No turns recorded yet
        </div>
      ) : viewMode === 'timeline' ? (
        <TimelineView
          turns={displayTurns}
          liveTurn={liveTurn}
          defaultExpandLastN={defaultExpandLastN}
          forced={forced}
          forcedSeq={forcedSeq}
          bottomRef={bottomRef}
        />
      ) : (
        <FlowView
          turns={displayTurns}
          liveTurn={liveTurn}
        />
      )}
    </div>
  )
}

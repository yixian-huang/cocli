import { useCallback, useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { agentSessions, agentTurns } from '@/api/client'
import type { AgentSession, Turn } from '@/lib/types'
import { cn } from '@/lib/utils'
import { Badge } from '@/components/ui'
import { Clock, Zap, DollarSign, RotateCcw, AlertCircle, Square, Loader2, ChevronDown, ChevronRight, ExternalLink } from 'lucide-react'
import { ContextBar } from './ContextBar'
import { useWorkspacePanelStore } from '@/stores/workspacePanelStore'
import { useZoneStore } from '@/stores/zoneStore'
import { messagePath } from '@/lib/paths'

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`
  return String(n)
}

function formatDuration(start: string, end?: string): string {
  const startMs = new Date(start).getTime()
  const endMs = end ? new Date(end).getTime() : Date.now()
  const diffSec = Math.floor((endMs - startMs) / 1000)
  if (diffSec < 60) return `${diffSec}s`
  if (diffSec < 3600) return `${Math.floor(diffSec / 60)}m ${diffSec % 60}s`
  const h = Math.floor(diffSec / 3600)
  const m = Math.floor((diffSec % 3600) / 60)
  return `${h}h ${m}m`
}

function formatTurnDuration(turn: Turn): string {
  if (turn.durationMs != null) {
    if (turn.durationMs < 1000) return `${turn.durationMs}ms`
    if (turn.durationMs < 60_000) return `${(turn.durationMs / 1000).toFixed(1)}s`
    return `${Math.floor(turn.durationMs / 60000)}m ${Math.round((turn.durationMs % 60000) / 1000)}s`
  }
  if (!turn.endedAt) return 'running'
  return formatDuration(turn.startedAt, turn.endedAt)
}

function getToolCallSequence(turn: Turn): string[] {
  if (turn.toolCalls && turn.toolCalls.length > 0) {
    return turn.toolCalls.map((call) => call.name)
  }
  const fromEntries = (turn.entries || [])
    .filter((entry) => entry.kind === 'tool_call')
    .map((entry) => (entry.input as { name?: string } | undefined)?.name)
    .filter(Boolean) as string[]
  return Array.from(new Set(fromEntries))
}

function reasonBadge(reason?: string) {
  if (!reason) {
    return (
      <Badge variant="success" size="sm" className="gap-1">
        <Loader2 className="h-3 w-3 animate-spin" /> Active
      </Badge>
    )
  }
  switch (reason) {
    case 'context_reset':
      return (
        <Badge variant="warning" size="sm" className="gap-1">
          <RotateCcw className="h-3 w-3" /> Context Reset
        </Badge>
      )
    case 'error':
      return (
        <Badge variant="error" size="sm" className="gap-1">
          <AlertCircle className="h-3 w-3" /> Error
        </Badge>
      )
    case 'manual_stop':
      return (
        <Badge size="sm" className="gap-1">
          <Square className="h-3 w-3" /> Stopped
        </Badge>
      )
    case 'idle':
      return (
        <Badge size="sm" className="gap-1">
          <Clock className="h-3 w-3" /> Idle
        </Badge>
      )
    default:
      return (
        <Badge variant="info" size="sm" className="gap-1">
          <Clock className="h-3 w-3" /> {reason}
        </Badge>
      )
  }
}

function TurnRow({
  turn,
  onJump,
}: {
  turn: Turn
  onJump: (turn: Turn) => void
}) {
  const tools = getToolCallSequence(turn)
  const totalTokens = (turn.inputTokens || 0) + (turn.outputTokens || 0)

  return (
    <div className="rounded-md border px-2.5 py-2 space-y-1.5 bg-background">
      <div className="flex items-center gap-2 text-xs">
        <span className="font-medium">Turn #{turn.turnNumber}</span>
        <span className="text-muted-foreground">{new Date(turn.startedAt).toLocaleString()}</span>
        <span className="ml-auto text-muted-foreground">{formatTurnDuration(turn)}</span>
      </div>
      <div className="text-[11px] text-muted-foreground flex flex-wrap gap-3">
        <span>in {turn.inputTokens ?? 0}</span>
        <span>out {turn.outputTokens ?? 0}</span>
        <span>total {totalTokens}</span>
        {turn.costUsd != null && turn.costUsd > 0 && <span>${turn.costUsd.toFixed(4)}</span>}
      </div>
      <div className="text-[11px] text-muted-foreground">
        tool calls: {tools.length > 0 ? tools.join(' → ') : 'none'}
      </div>
      {turn.messageRef && (
        <button
          onClick={() => onJump(turn)}
          className="text-[11px] inline-flex items-center gap-1 text-primary hover:text-primary/80"
        >
          <ExternalLink className="h-3 w-3" />
          Jump to message
        </button>
      )}
    </div>
  )
}

export function SessionsTab({ agentId }: { agentId: string }) {
  const navigate = useNavigate()
  const zoneSlug = useZoneStore((s) => s.activeZoneSlug)
  const setWorkspacePanel = useWorkspacePanelStore((s) => s.setPanel)
  const [chatSessions, setChatSessions] = useState<AgentSession[]>([])
  const [loading, setLoading] = useState(true)
  const [expandedSessionId, setExpandedSessionId] = useState<string | null>(null)
  const [turnsBySession, setTurnsBySession] = useState<Record<string, Turn[]>>({})
  const [loadingTurnsBySession, setLoadingTurnsBySession] = useState<Record<string, boolean>>({})
  const [turnErrorBySession, setTurnErrorBySession] = useState<Record<string, string | null>>({})

  const loadSessionTurns = useCallback(async (sessionId: string) => {
    setLoadingTurnsBySession((prev) => ({ ...prev, [sessionId]: true }))
    setTurnErrorBySession((prev) => ({ ...prev, [sessionId]: null }))
    try {
      let turns: Turn[] = []
      try {
        turns = await agentTurns.listBySession(agentId, sessionId, 120, 0)
      } catch {
        turns = await agentTurns.list(agentId, sessionId, 120, 0)
      }
      setTurnsBySession((prev) => ({ ...prev, [sessionId]: turns || [] }))
    } catch (err) {
      setTurnErrorBySession((prev) => ({
        ...prev,
        [sessionId]: err instanceof Error ? err.message : 'Failed to load turns',
      }))
    } finally {
      setLoadingTurnsBySession((prev) => ({ ...prev, [sessionId]: false }))
    }
  }, [agentId])

  useEffect(() => {
    let cancelled = false
    setLoading(true)
    agentSessions.list(agentId, 50, 'chat').then((chat) => {
      if (!cancelled) {
        setChatSessions(chat || [])
        setLoading(false)
      }
    }).catch(() => {
      if (!cancelled) setLoading(false)
    })
    return () => { cancelled = true }
  }, [agentId])

  useEffect(() => {
    if (!expandedSessionId) return
    if (turnsBySession[expandedSessionId]) return
    if (loadingTurnsBySession[expandedSessionId]) return
    loadSessionTurns(expandedSessionId)
  }, [expandedSessionId, turnsBySession, loadingTurnsBySession, loadSessionTurns])

  if (loading) {
    return (
      <div className="flex-1 flex items-center justify-center text-muted-foreground text-sm">
        <Loader2 className="h-4 w-4 animate-spin mr-2" /> Loading sessions...
      </div>
    )
  }

  if (chatSessions.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center text-muted-foreground text-sm">
        No session history
      </div>
    )
  }

  // Aggregate stats for chat sessions
  const totalCost = chatSessions.reduce((s, sess) => s + (sess.costUsd || 0), 0)
  const totalOutput = chatSessions.reduce((s, sess) => s + (sess.outputTokens || 0), 0)
  const totalTurns = chatSessions.reduce((s, sess) => s + (sess.turnCount || 0), 0)
  const resets = chatSessions.filter((s) => s.endReason === 'context_reset').length

  return (
    <div className="flex-1 overflow-y-auto">
      {/* Chat Sessions */}
      {chatSessions.length > 0 && (
        <>
          {/* Summary bar */}
          <div className="sticky top-0 bg-background border-b px-4 py-2 flex items-center gap-4 text-xs text-muted-foreground">
            <span>{chatSessions.length} sessions</span>
            <span className="flex items-center gap-1"><Zap className="h-3 w-3" />{totalTurns} turns</span>
            <span className="flex items-center gap-1">{formatTokens(totalOutput)} output</span>
            {totalCost > 0 && (
              <span className="flex items-center gap-1"><DollarSign className="h-3 w-3" />${totalCost.toFixed(3)}</span>
            )}
            {resets > 0 && (
              <span className="flex items-center gap-1"><RotateCcw className="h-3 w-3" />{resets} resets</span>
            )}
          </div>

          {/* Session list */}
          <div className="divide-y">
            {chatSessions.map((sess) => {
              const pct = sess.contextWindow > 0 && sess.inputTokens > 0
                ? Math.round((sess.inputTokens / sess.contextWindow) * 100)
                : null
              const sessionKey = sess.sessionId || sess.id
              const expanded = expandedSessionId === sessionKey
              const sessionTurns = turnsBySession[sessionKey] || []
              const turnsLoading = !!loadingTurnsBySession[sessionKey]
              const turnError = turnErrorBySession[sessionKey]

              return (
                <div key={sess.id} className="px-4 py-3 hover:bg-accent/30 transition-colors">
                  <div className="flex items-center justify-between mb-1.5">
                    <div className="flex items-center gap-2">
                      {reasonBadge(sess.endReason)}
                      <span className="text-xs text-muted-foreground">
                        {new Date(sess.startedAt).toLocaleString()}
                      </span>
                    </div>
                    <span className="text-xs text-muted-foreground">
                      {formatDuration(sess.startedAt, sess.endedAt)}
                    </span>
                  </div>
                  <div className="mb-2">
                    <button
                      className="inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground"
                      onClick={() =>
                        setExpandedSessionId((prev) => (prev === sessionKey ? null : sessionKey))
                      }
                    >
                      {expanded ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
                      {expanded ? 'Hide turns' : 'Show turns'}
                    </button>
                  </div>

                  <div className="flex items-center gap-4 text-[11px] text-muted-foreground/70">
                    <span>{sess.turnCount} turns</span>
                    {sess.inputTokens > 0 && (
                      <span>{formatTokens(sess.inputTokens)} input</span>
                    )}
                    {sess.outputTokens > 0 && (
                      <span>{formatTokens(sess.outputTokens)} output</span>
                    )}
                    {sess.costUsd > 0 && (
                      <span>${sess.costUsd.toFixed(3)}</span>
                    )}
                    {pct != null && (
                      <span className={cn(
                        'font-medium',
                        pct >= 80 ? 'text-red-500' : pct >= 50 ? 'text-amber-500' : 'text-emerald-500',
                      )}>
                        {pct}% context
                      </span>
                    )}
                  </div>

                  {/* Mini context bar */}
                  {pct != null && (
                    <div className="mt-2">
                      <div className="flex justify-between text-xs text-muted-foreground/70">
                        <span>Context</span>
                        <span>{pct}%</span>
                      </div>
                      <div className="h-1.5 rounded-full bg-muted overflow-hidden mt-1">
                        <div
                          className={cn(
                            'h-full rounded-full transition-all',
                            pct >= 80 ? 'bg-red-500' : pct >= 50 ? 'bg-amber-500' : 'bg-emerald-500',
                          )}
                          style={{ width: `${Math.min(pct, 100)}%` }}
                        />
                      </div>
                    </div>
                  )}
                  {!sess.endedAt && sess.contextWindow > 0 && sess.inputTokens > 0 && (
                    <ContextBar
                      lastInputTokens={sess.inputTokens}
                      contextWindow={sess.contextWindow}
                      totalOutputTokens={sess.outputTokens}
                      totalCostUSD={sess.costUsd}
                      turnCount={sess.turnCount}
                      className="mt-2"
                    />
                  )}

                  {expanded && (
                    <div className="mt-3 space-y-2 border-t pt-3">
                      {turnsLoading ? (
                        <div className="text-xs text-muted-foreground flex items-center gap-1">
                          <Loader2 className="h-3.5 w-3.5 animate-spin" />
                          Loading turns...
                        </div>
                      ) : turnError ? (
                        <div className="text-xs text-red-500">{turnError}</div>
                      ) : sessionTurns.length === 0 ? (
                        <div className="text-xs text-muted-foreground">No turns found for this session</div>
                      ) : (
                        sessionTurns
                          .slice()
                          .sort((a, b) => new Date(a.startedAt).getTime() - new Date(b.startedAt).getTime())
                          .map((turn) => (
                            <TurnRow
                              key={turn.id || `${sessionKey}-${turn.turnNumber}`}
                              turn={turn}
                              onJump={(targetTurn) => {
                                if (!targetTurn.messageRef) return
                                setWorkspacePanel('chat')
                                navigate(messagePath({ zoneSlug, channelId: targetTurn.messageRef.channelId, messageId: targetTurn.messageRef.messageId }))
                                window.dispatchEvent(new CustomEvent('scroll-to-message', { detail: { msgId: targetTurn.messageRef.messageId } }))
                              }}
                            />
                          ))
                      )}
                    </div>
                  )}
                </div>
              )
            })}
          </div>
        </>
      )}
    </div>
  )
}

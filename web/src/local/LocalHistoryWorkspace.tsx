import {
  Activity,
  Bot,
  ChevronDown,
  ChevronRight,
  CircleDollarSign,
  Clock3,
  ExternalLink,
  MessageSquareText,
  RefreshCw,
  RotateCcw,
  Timer,
  Wrench,
  Zap,
} from 'lucide-react'
import {
  useCallback,
  useEffect,
  useMemo,
  useState,
} from 'react'
import {
  localApi,
  type Agent,
  type Channel,
  type RuntimeActivity,
  type RuntimeSession,
  type RuntimeTrajectoryEntry,
  type RuntimeTurn,
} from './api'
import { LocalSelect } from './LocalSelect'
import type { LocalCopyKey } from './localization'
import './LocalHistoryWorkspace.css'

interface LocalHistoryWorkspaceProps {
  agents: Agent[]
  channels: Channel[]
  onOpenMessage: (channelId: string, messageId: string) => void
  t: (key: LocalCopyKey, values?: Record<string, string | number>) => string
}

function formatDate(value: string): string {
  const date = new Date(value)
  return Number.isNaN(date.getTime())
    ? value
    : date.toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' })
}

function formatDuration(startedAt: string, endedAt?: string, durationMs?: number): string {
  const milliseconds = durationMs ?? (
    Math.max(0, new Date(endedAt ?? Date.now()).getTime() - new Date(startedAt).getTime())
  )
  if (milliseconds < 1_000) return `${milliseconds}ms`
  if (milliseconds < 60_000) return `${(milliseconds / 1_000).toFixed(1)}s`
  const minutes = Math.floor(milliseconds / 60_000)
  const seconds = Math.round((milliseconds % 60_000) / 1_000)
  return `${minutes}m ${seconds}s`
}

function formatTokens(value: number): string {
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`
  if (value >= 1_000) return `${(value / 1_000).toFixed(1)}k`
  return String(value)
}

function sessionStatus(session: RuntimeSession, t: LocalHistoryWorkspaceProps['t']): string {
  if (!session.endedAt) return t('historyActive')
  switch (session.endReason) {
    case 'context_reset':
      return t('historyContextReset')
    case 'manual_stop':
      return t('historyStopped')
    case 'error':
      return t('historyError')
    case 'idle':
      return t('historyIdle')
    default:
      return session.endReason || t('historyCompleted')
  }
}

function entryLabel(entry: RuntimeTrajectoryEntry, t: LocalHistoryWorkspaceProps['t']): string {
  switch (entry.kind) {
    case 'tool_call':
      return typeof entry.input?.name === 'string' ? entry.input.name : t('historyToolCall')
    case 'tool_result':
      return entry.error ? t('historyToolError') : t('historyToolResult')
    case 'thinking':
      return t('historyThinking')
    case 'input':
      return t('historyInput')
    case 'text':
      return t('historyOutput')
    case 'warning':
      return t('historyWarning')
    case 'error':
      return t('historyError')
    case 'status':
      return t('historyStatus')
  }
}

function entryBody(entry: RuntimeTrajectoryEntry): string {
  if (entry.text) return entry.text
  if (entry.error) return entry.error
  if (entry.result) return entry.result
  if (entry.input) {
    const input = { ...entry.input }
    delete input.name
    return Object.keys(input).length === 0 ? '' : JSON.stringify(input, null, 2)
  }
  return ''
}

function HistoryEntry({
  entry,
  t,
}: {
  entry: RuntimeTrajectoryEntry
  t: LocalHistoryWorkspaceProps['t']
}) {
  const body = entryBody(entry)
  return (
    <div className={`history-entry history-entry-${entry.kind}`}>
      <span className="history-entry-marker" aria-hidden="true">
        {entry.kind === 'tool_call' || entry.kind === 'tool_result'
          ? <Wrench size={12} />
          : entry.kind === 'input'
            ? <MessageSquareText size={12} />
            : <Activity size={12} />}
      </span>
      <div>
        <strong>{entryLabel(entry, t)}</strong>
        {body && <pre>{body}</pre>}
      </div>
    </div>
  )
}

export function LocalHistoryWorkspace({
  agents,
  channels,
  onOpenMessage,
  t,
}: LocalHistoryWorkspaceProps) {
  const [agentId, setAgentId] = useState(agents[0]?.id ?? '')
  const [view, setView] = useState<'sessions' | 'activity'>('sessions')
  const [sessions, setSessions] = useState<RuntimeSession[]>([])
  const [currentSession, setCurrentSession] = useState<RuntimeSession | null>(null)
  const [turns, setTurns] = useState<RuntimeTurn[]>([])
  const [activities, setActivities] = useState<RuntimeActivity[]>([])
  const [selectedSessionId, setSelectedSessionId] = useState('')
  const [expandedTurnId, setExpandedTurnId] = useState('')
  const [query, setQuery] = useState('')
  const [loading, setLoading] = useState(false)
  const [turnsLoading, setTurnsLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const selectedAgent = useMemo(
    () => agents.find((agent) => agent.id === agentId) ?? null,
    [agentId, agents],
  )
  const agentOptions = useMemo(
    () => agents.map((agent) => ({
      value: agent.id,
      label: agent.name,
      meta: channels.find((channel) => channel.id === agent.channel_id)?.name,
    })),
    [agents, channels],
  )
  const selectedSession = useMemo(
    () => sessions.find((session) => session.sessionId === selectedSessionId) ?? null,
    [selectedSessionId, sessions],
  )
  const summary = useMemo(() => ({
    turns: sessions.reduce((total, session) => total + session.turnCount, 0),
    outputTokens: sessions.reduce((total, session) => total + session.outputTokens, 0),
    cost: sessions.reduce((total, session) => total + session.costUsd, 0),
    resets: sessions.filter((session) => session.endReason === 'context_reset').length,
  }), [sessions])
  const visibleActivities = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase()
    if (!normalized) return activities
    return activities.filter((entry) => (
      entry.activity.toLocaleLowerCase().includes(normalized)
      || entry.detail?.toLocaleLowerCase().includes(normalized)
      || entry.trajectory.some((item) => item.toLocaleLowerCase().includes(normalized))
      || entry.sessionId?.toLocaleLowerCase().includes(normalized)
    ))
  }, [activities, query])

  const loadHistory = useCallback(async () => {
    if (!selectedAgent) {
      setSessions([])
      setCurrentSession(null)
      setActivities([])
      return
    }
    setLoading(true)
    setError(null)
    try {
      const [nextSessions, nextCurrent, nextActivities] = await Promise.all([
        localApi.listRuntimeSessions(selectedAgent.id),
        localApi.getCurrentRuntimeSession(selectedAgent.id),
        localApi.listRuntimeActivity(selectedAgent.id),
      ])
      setSessions(nextSessions)
      setCurrentSession(nextCurrent)
      setActivities(nextActivities)
      setSelectedSessionId((current) => (
        nextSessions.some((session) => session.sessionId === current)
          ? current
          : nextCurrent?.sessionId ?? nextSessions[0]?.sessionId ?? ''
      ))
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : t('historyLoadError'))
    } finally {
      setLoading(false)
    }
  }, [selectedAgent, t])

  useEffect(() => {
    setAgentId((current) => (
      agents.some((agent) => agent.id === current) ? current : agents[0]?.id ?? ''
    ))
  }, [agents])

  useEffect(() => {
    setSelectedSessionId('')
    setExpandedTurnId('')
    void loadHistory()
  }, [loadHistory])

  useEffect(() => {
    if (!selectedAgent || !selectedSessionId) {
      setTurns([])
      return
    }
    let cancelled = false
    setTurnsLoading(true)
    localApi.listRuntimeTurns(selectedAgent.id, selectedSessionId)
      .then((nextTurns) => {
        if (!cancelled) setTurns(nextTurns)
      })
      .catch((nextError: unknown) => {
        if (!cancelled) {
          setError(nextError instanceof Error ? nextError.message : t('historyLoadError'))
        }
      })
      .finally(() => {
        if (!cancelled) setTurnsLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [selectedAgent, selectedSessionId, t])

  if (agents.length === 0) {
    return (
      <section className="local-history-workspace">
        <div className="history-empty">
          <Bot size={28} aria-hidden="true" />
          <h1>{t('historyNoAgent')}</h1>
          <p>{t('historyNoAgentDescription')}</p>
        </div>
      </section>
    )
  }

  return (
    <section className="local-history-workspace">
      <header className="history-hero">
        <div>
          <span className="workspace-eyebrow">{t('historyEyebrow')}</span>
          <h1>{t('historyTitle')}</h1>
          <p>{t('historyDescription')}</p>
        </div>
        <div className="history-hero-actions">
          <LocalSelect
            id="history-agent"
            ariaLabel={t('historyAgent')}
            value={agentId}
            options={agentOptions}
            onChange={setAgentId}
            placeholder={t('historySelectAgent')}
          />
          <button type="button" aria-label={t('refresh')} onClick={() => void loadHistory()}>
            <RefreshCw size={15} aria-hidden="true" />
          </button>
        </div>
      </header>

      {error && <div className="workspace-error" role="alert">{error}</div>}

      <div className="history-summary">
        <article>
          <Clock3 size={15} aria-hidden="true" />
          <span>{t('historySessions')}</span>
          <strong>{sessions.length}</strong>
          <small>{currentSession ? t('historyOneActive') : t('historyNoneActive')}</small>
        </article>
        <article>
          <Zap size={15} aria-hidden="true" />
          <span>{t('historyTurns')}</span>
          <strong>{summary.turns}</strong>
          <small>{formatTokens(summary.outputTokens)} {t('historyOutputTokens')}</small>
        </article>
        <article>
          <CircleDollarSign size={15} aria-hidden="true" />
          <span>{t('historyCost')}</span>
          <strong>${summary.cost.toFixed(4)}</strong>
          <small>{t('historyLocalUsage')}</small>
        </article>
        <article>
          <RotateCcw size={15} aria-hidden="true" />
          <span>{t('historyResets')}</span>
          <strong>{summary.resets}</strong>
          <small>{t('historyContextResets')}</small>
        </article>
      </div>

      <div className="history-shell">
        <div className="history-toolbar">
          <div className="segmented-control" aria-label={t('historyView')}>
            <button
              type="button"
              className={view === 'sessions' ? 'active' : ''}
              onClick={() => setView('sessions')}
            >
              <Clock3 size={13} aria-hidden="true" />
              {t('historySessions')}
            </button>
            <button
              type="button"
              className={view === 'activity' ? 'active' : ''}
              onClick={() => setView('activity')}
            >
              <Activity size={13} aria-hidden="true" />
              {t('historyActivity')}
            </button>
          </div>
          {view === 'activity' && (
            <input
              aria-label={t('historyFilterActivity')}
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder={t('historyFilterPlaceholder')}
            />
          )}
        </div>

        {loading ? (
          <div className="history-empty"><p>{t('historyLoading')}</p></div>
        ) : view === 'sessions' ? (
          <div className="history-session-layout">
            <aside className="history-session-list">
              {sessions.length === 0 ? (
                <div className="history-empty compact">
                  <p>{t('historyEmptySessions')}</p>
                </div>
              ) : sessions.map((session) => {
                const selected = session.sessionId === selectedSessionId
                const contextPercent = session.contextWindow > 0
                  ? Math.round((session.inputTokens / session.contextWindow) * 100)
                  : 0
                return (
                  <button
                    type="button"
                    key={session.id}
                    className={selected ? 'active' : ''}
                    onClick={() => {
                      setSelectedSessionId(session.sessionId)
                      setExpandedTurnId('')
                    }}
                  >
                    <span className={`session-status ${session.endedAt ? 'ended' : 'active'}`}>
                      {sessionStatus(session, t)}
                    </span>
                    <strong>{formatDate(session.startedAt)}</strong>
                    <small>{session.sessionType} · {session.turnCount} {t('historyTurnsLower')}</small>
                    <div className="session-context">
                      <span style={{ width: `${Math.min(contextPercent, 100)}%` }} />
                    </div>
                    <small>{contextPercent}% {t('historyContext')}</small>
                  </button>
                )
              })}
            </aside>

            <section className="history-turns">
              {!selectedSession ? (
                <div className="history-empty compact"><p>{t('historySelectSession')}</p></div>
              ) : (
                <>
                  <header className="history-session-header">
                    <div>
                      <span className="workspace-eyebrow">{selectedSession.sessionId}</span>
                      <h2>{selectedSession.taskSummary || t('historySession')}</h2>
                      <p>
                        {formatDate(selectedSession.startedAt)}
                        {' · '}
                        {formatDuration(selectedSession.startedAt, selectedSession.endedAt)}
                      </p>
                    </div>
                    <dl>
                      <div><dt>{t('historyInput')}</dt><dd>{formatTokens(selectedSession.inputTokens)}</dd></div>
                      <div><dt>{t('historyOutput')}</dt><dd>{formatTokens(selectedSession.outputTokens)}</dd></div>
                      <div><dt>{t('historyCost')}</dt><dd>${selectedSession.costUsd.toFixed(4)}</dd></div>
                    </dl>
                  </header>
                  <div className="history-turn-list">
                    {turnsLoading ? (
                      <div className="history-empty compact"><p>{t('historyLoadingTurns')}</p></div>
                    ) : turns.length === 0 ? (
                      <div className="history-empty compact"><p>{t('historyEmptyTurns')}</p></div>
                    ) : turns.map((turn) => {
                      const expanded = expandedTurnId === turn.id
                      return (
                        <article className={`history-turn ${expanded ? 'expanded' : ''}`} key={turn.id}>
                          <button
                            type="button"
                            className="history-turn-summary"
                            onClick={() => setExpandedTurnId(expanded ? '' : turn.id)}
                            aria-expanded={expanded}
                          >
                            {expanded
                              ? <ChevronDown size={14} aria-hidden="true" />
                              : <ChevronRight size={14} aria-hidden="true" />}
                            <strong>{t('historyTurnNumber', { number: turn.turnNumber })}</strong>
                            <span>{formatDate(turn.startedAt)}</span>
                            <span className="turn-duration"><Timer size={12} />{formatDuration(
                              turn.startedAt,
                              turn.endedAt,
                              turn.durationMs,
                            )}</span>
                            <span>{formatTokens(turn.inputTokens)} → {formatTokens(turn.outputTokens)}</span>
                          </button>
                          {expanded && (
                            <div className="history-turn-detail">
                              <div className="history-entry-list">
                                {turn.entries.length === 0
                                  ? <p>{t('historyNoTrajectory')}</p>
                                  : turn.entries.map((entry, index) => (
                                    <HistoryEntry
                                      key={entry.id ?? `${turn.id}-${index}`}
                                      entry={entry}
                                      t={t}
                                    />
                                  ))}
                              </div>
                              {turn.messageRef && (
                                <button
                                  type="button"
                                  className="history-message-link"
                                  onClick={() => onOpenMessage(
                                    turn.messageRef!.channelId,
                                    turn.messageRef!.messageId,
                                  )}
                                >
                                  <ExternalLink size={13} aria-hidden="true" />
                                  {t('historyOpenMessage', {
                                    seq: turn.messageRef.seq ?? '?',
                                  })}
                                </button>
                              )}
                            </div>
                          )}
                        </article>
                      )
                    })}
                  </div>
                </>
              )}
            </section>
          </div>
        ) : (
          <div className="history-activity-list">
            {visibleActivities.length === 0 ? (
              <div className="history-empty compact">
                <p>{t(activities.length === 0 ? 'historyEmptyActivity' : 'historyNoMatches')}</p>
              </div>
            ) : visibleActivities.map((entry) => (
              <article key={entry.id}>
                <span className="activity-dot" aria-hidden="true" />
                <div className="activity-time">
                  <time dateTime={entry.createdAt}>{formatDate(entry.createdAt)}</time>
                  {entry.sessionId && <small>{entry.sessionId}</small>}
                </div>
                <div className="activity-content">
                  <strong>{entry.activity}</strong>
                  {entry.detail && <p>{entry.detail}</p>}
                  {entry.trajectory.length > 0 && (
                    <div>{entry.trajectory.map((item) => <span key={item}>{item}</span>)}</div>
                  )}
                </div>
              </article>
            ))}
          </div>
        )}
      </div>
    </section>
  )
}

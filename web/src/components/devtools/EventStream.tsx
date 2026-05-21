import { useEffect, useRef, useMemo } from 'react'
import { useDevToolsStore } from '@/stores/devToolsStore'

const TYPE_COLORS: Record<string, string> = {
  'agent:session': 'bg-green-500/10 text-green-400',
  'agent:session:end': 'bg-red-500/10 text-red-400',
  'agent:session:idle': 'bg-amber-500/10 text-amber-400',
  'agent:turn': 'bg-blue-500/10 text-blue-400',
  'agent:deliver:ack': 'bg-zinc-500/10 text-zinc-400',
  'agent:activity': 'bg-purple-500/10 text-purple-400',
  'agent:prompt:info': 'bg-pink-500/10 text-pink-400',
  'agent:status': 'bg-cyan-500/10 text-cyan-400',
}

function formatTime(ts: number): string {
  const d = new Date(ts)
  return d.toLocaleTimeString('en-US', {
    hour12: false,
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  })
}

function truncate(s: string, max: number): string {
  return s.length > max ? s.slice(0, max) + '...' : s
}

function dataSnippet(type: string, data: Record<string, unknown>): string {
  const parts: string[] = []
  if (data.costUsd != null) parts.push(`$${Number(data.costUsd).toFixed(4)}`)
  if (data.inputTokens != null || data.outputTokens != null) {
    const inp = data.inputTokens ?? 0
    const out = data.outputTokens ?? 0
    parts.push(`${inp}/${out} tok`)
  }
  if (data.routeAction != null) parts.push(`route:${data.routeAction}`)
  if (data.status != null) parts.push(`${data.status}`)
  if (data.activeSessions != null) parts.push(`active:${data.activeSessions}`)
  if (data.endReason != null) parts.push(`reason:${data.endReason}`)
  if (parts.length === 0 && type === 'agent:deliver:ack') parts.push('ack')
  return parts.join(' | ')
}

export function EventStream() {
  const events = useDevToolsStore((s) => s.events)
  const filters = useDevToolsStore((s) => s.filters)
  const isPaused = useDevToolsStore((s) => s.isPaused)

  const filteredEvents = useMemo(() => {
    return events.filter((e) => {
      if (filters.agentId && e.agentId !== filters.agentId) return false
      if (filters.eventType && e.type !== filters.eventType) return false
      if (filters.channelName && e.channelName !== filters.channelName) return false
      return true
    })
  }, [events, filters])
  const bottomRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!isPaused && bottomRef.current) {
      bottomRef.current.scrollIntoView({ behavior: 'smooth' })
    }
  }, [filteredEvents.length, isPaused])

  if (filteredEvents.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
        No events yet
      </div>
    )
  }

  return (
    <div className="font-mono text-[11px] leading-relaxed">
      {filteredEvents.map((event) => {
        const colorClass = TYPE_COLORS[event.type] ?? 'bg-zinc-500/10 text-zinc-400'
        const snippet = dataSnippet(event.type, event.data)
        const label = event.agentName || truncate(event.agentId, 8)

        return (
          <div
            key={event.id}
            className="flex items-center gap-2 px-3 py-1 hover:bg-accent/30 border-b border-zinc-800/50"
          >
            <span className="text-muted-foreground shrink-0">
              {formatTime(event.timestamp)}
            </span>
            <span className="text-foreground shrink-0 w-20 truncate" title={event.agentId}>
              {label}
            </span>
            {event.channelName && (
              <span className="text-muted-foreground shrink-0 w-20 truncate" title={event.channelName}>
                #{event.channelName}
              </span>
            )}
            <span className={`shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium ${colorClass}`}>
              {event.type}
            </span>
            {snippet && (
              <span className="text-muted-foreground truncate">{snippet}</span>
            )}
          </div>
        )
      })}
      <div ref={bottomRef} />
    </div>
  )
}

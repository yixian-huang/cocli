import { useMemo } from 'react'
import { useDevToolsStore } from '@/stores/devToolsStore'

interface AgentCapacity {
  agentId: string
  agentName: string
  activeSessions: number
  maxConcurrent: number
}

export function CapacityOverview() {
  const events = useDevToolsStore((s) => s.events)

  const agents = useMemo(() => {
    const map = new Map<string, AgentCapacity>()

    for (const event of events) {
      if (event.type !== 'agent:session' && event.type !== 'agent:session:idle') continue

      map.set(event.agentId, {
        agentId: event.agentId,
        agentName: event.agentName || event.agentId.slice(0, 8),
        activeSessions: Number(event.data.activeSessions ?? 0),
        maxConcurrent: Number(event.data.maxConcurrent ?? 1),
      })
    }

    return Array.from(map.values())
  }, [events])

  if (agents.length === 0) {
    return (
      <div>
        <h2 className="text-xs font-semibold text-muted-foreground uppercase tracking-wide mb-2">
          Capacity
        </h2>
        <p className="text-xs text-muted-foreground">No session data</p>
      </div>
    )
  }

  return (
    <div>
      <h2 className="text-xs font-semibold text-muted-foreground uppercase tracking-wide mb-3">
        Capacity
      </h2>
      <div className="space-y-3">
        {agents.map((agent) => {
          const max = Math.max(agent.maxConcurrent, 1)
          const active = Math.min(agent.activeSessions, max)

          return (
            <div key={agent.agentId}>
              <div className="flex items-center justify-between mb-1">
                <span className="text-xs text-foreground truncate" title={agent.agentId}>
                  {agent.agentName}
                </span>
                <span className="text-[10px] text-muted-foreground">
                  {active}/{max}
                </span>
              </div>
              <div className="flex gap-0.5">
                {Array.from({ length: max }, (_, i) => (
                  <div
                    key={i}
                    className={`h-2 flex-1 rounded-sm ${
                      i < active
                        ? 'bg-green-500'
                        : 'border border-border bg-transparent'
                    }`}
                  />
                ))}
              </div>
            </div>
          )
        })}
      </div>
    </div>
  )
}

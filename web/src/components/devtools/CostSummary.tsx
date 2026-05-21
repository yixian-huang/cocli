import { useMemo } from 'react'
import { useDevToolsStore } from '@/stores/devToolsStore'

interface AgentCost {
  agentId: string
  agentName: string
  totalCost: number
  totalTurns: number
}

export function CostSummary() {
  const events = useDevToolsStore((s) => s.events)

  const agents = useMemo(() => {
    const map = new Map<string, AgentCost>()

    for (const event of events) {
      if (event.type !== 'agent:turn') continue

      const existing = map.get(event.agentId)
      const cost = Number(event.data.costUsd ?? 0)

      if (existing) {
        existing.totalCost += cost
        existing.totalTurns += 1
      } else {
        map.set(event.agentId, {
          agentId: event.agentId,
          agentName: event.agentName || event.agentId.slice(0, 8),
          totalCost: cost,
          totalTurns: 1,
        })
      }
    }

    return Array.from(map.values())
  }, [events])

  if (agents.length === 0) {
    return (
      <div>
        <h2 className="text-xs font-semibold text-muted-foreground uppercase tracking-wide mb-2">
          Cost
        </h2>
        <p className="text-xs text-muted-foreground">No turn data</p>
      </div>
    )
  }

  return (
    <div>
      <h2 className="text-xs font-semibold text-muted-foreground uppercase tracking-wide mb-3">
        Cost
      </h2>
      <table className="w-full text-xs">
        <thead>
          <tr className="text-muted-foreground text-left">
            <th className="pb-1 font-medium">Agent</th>
            <th className="pb-1 font-medium text-right">Cost</th>
            <th className="pb-1 font-medium text-right">Turns</th>
          </tr>
        </thead>
        <tbody>
          {agents.map((agent) => (
            <tr key={agent.agentId} className="border-t border-border/50">
              <td className="py-1 text-foreground truncate max-w-[120px]" title={agent.agentId}>
                {agent.agentName}
              </td>
              <td className="py-1 text-right text-muted-foreground">
                ${agent.totalCost.toFixed(4)}
              </td>
              <td className="py-1 text-right text-muted-foreground">
                {agent.totalTurns}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

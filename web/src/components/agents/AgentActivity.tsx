import { useState, useMemo, useCallback } from 'react'
import { useNavigate } from 'react-router-dom'
import { useAgentStore } from '@/stores/agentStore'
import { useChannelStore } from '@/stores/channelStore'
import { useViewStore } from '@/stores/viewStore'
import { useZoneStore } from '@/stores/zoneStore'
import { cn } from '@/lib/utils'
import { Bot, ChevronDown, ChevronRight, Loader2 } from 'lucide-react'
import { agentPath } from '@/lib/paths'

function BouncingDots() {
  return (
    <span className="flex gap-0.5 ml-1">
      <span className="h-1 w-1 rounded-full bg-primary animate-bounce" style={{ animationDelay: '0ms' }} />
      <span className="h-1 w-1 rounded-full bg-primary animate-bounce" style={{ animationDelay: '150ms' }} />
      <span className="h-1 w-1 rounded-full bg-primary animate-bounce" style={{ animationDelay: '300ms' }} />
    </span>
  )
}

export function AgentActivity() {
  const navigate = useNavigate()
  const agents = useAgentStore((s) => s.agents)
  const activeChannelId = useChannelStore((s) => s.activeChannelId)
  const members = useChannelStore((s) => activeChannelId ? s.membersByChannel[activeChannelId] : undefined)
  const setActiveAgent = useViewStore((s) => s.setActiveAgent)
  const zoneSlug = useZoneStore((s) => s.activeZoneSlug)
  const [expanded, setExpanded] = useState(false)

  const memberIds = useMemo(() => {
    if (!members) return null
    return new Set(members.map((m) => m.memberId))
  }, [members])

  const workingAgents = useMemo(() => {
    return agents.filter((a) => {
      if (a.status !== 'working') return false
      if (memberIds && !memberIds.has(a.id)) return false
      return true
    })
  }, [agents, memberIds])

  const handleAgentClick = useCallback((agentId: string) => {
    setActiveAgent(agentId)
    navigate(agentPath({ zoneSlug, agentId }))
  }, [navigate, setActiveAgent, zoneSlug])

  if (workingAgents.length === 0) return null

  // Single agent: inline with click to navigate
  if (workingAgents.length === 1) {
    const agent = workingAgents[0]
    return (
      <div className="border-t px-4 py-1.5">
        <button
          onClick={() => handleAgentClick(agent.id)}
          className="flex items-center gap-2 text-xs text-muted-foreground hover:text-foreground transition-colors w-full"
        >
          <Loader2 className="h-3 w-3 animate-spin text-primary shrink-0" />
          <Bot className="h-3 w-3 shrink-0" />
          <span className={cn('font-medium', 'text-primary')}>@{agent.name}</span>
          {agent.detail && <span className="truncate">{agent.detail}</span>}
          <BouncingDots />
        </button>
      </div>
    )
  }

  // Multiple agents: collapsible
  return (
    <div className="border-t px-4 py-1.5">
      <button
        onClick={() => setExpanded((v) => !v)}
        className="flex items-center gap-2 text-xs text-muted-foreground hover:text-foreground transition-colors w-full"
      >
        <Loader2 className="h-3 w-3 animate-spin text-primary shrink-0" />
        {expanded ? <ChevronDown className="h-3 w-3 shrink-0" /> : <ChevronRight className="h-3 w-3 shrink-0" />}
        <span className="font-medium text-primary">{workingAgents.length} agents working</span>
        {!expanded && (
          <span className="truncate">
            {workingAgents.map((a) => `@${a.name}`).join(', ')}
          </span>
        )}
        <BouncingDots />
      </button>
      {expanded && (
        <div className="mt-1 space-y-0.5 pl-5">
          {workingAgents.map((agent) => (
            <button
              key={agent.id}
              onClick={() => handleAgentClick(agent.id)}
              className="flex items-center gap-2 text-xs text-muted-foreground hover:text-foreground transition-colors w-full"
            >
              <Bot className="h-3 w-3 shrink-0" />
              <span className={cn('font-medium', 'text-primary')}>@{agent.name}</span>
              {agent.detail && <span className="truncate">{agent.detail}</span>}
            </button>
          ))}
        </div>
      )}
    </div>
  )
}

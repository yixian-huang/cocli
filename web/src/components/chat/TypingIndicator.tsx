import { useMemo } from 'react'
import { useAgentStore } from '@/stores/agentStore'
import { useChannelStore } from '@/stores/channelStore'
import { Bot } from 'lucide-react'

export function TypingIndicator() {
  const agents = useAgentStore((s) => s.agents)
  const activeChannelId = useChannelStore((s) => s.activeChannelId)
  const members = useChannelStore((s) => activeChannelId ? s.membersByChannel[activeChannelId] : undefined)

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

  if (workingAgents.length === 0) return null

  return (
    <div className="flex items-center gap-2 px-4 py-1.5 text-xs text-muted-foreground animate-in fade-in slide-in-from-bottom-1 duration-200">
      <Bot className="h-3 w-3 text-primary shrink-0" />
      <span>
        {workingAgents.map((a) => `@${a.name}`).join(', ')}{' '}
        {workingAgents.length === 1 ? 'is' : 'are'} working
      </span>
      {workingAgents.length === 1 && workingAgents[0].detail && (
        <span className="text-muted-foreground/50 truncate">
          &middot; {workingAgents[0].detail}
        </span>
      )}
      <span className="flex gap-0.5 ml-1">
        <span className="h-1 w-1 rounded-full bg-primary animate-bounce" style={{ animationDelay: '0ms' }} />
        <span className="h-1 w-1 rounded-full bg-primary animate-bounce" style={{ animationDelay: '150ms' }} />
        <span className="h-1 w-1 rounded-full bg-primary animate-bounce" style={{ animationDelay: '300ms' }} />
      </span>
    </div>
  )
}

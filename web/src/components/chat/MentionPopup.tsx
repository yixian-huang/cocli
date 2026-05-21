import { useEffect, useRef, useMemo } from 'react'
import { useAgentStore } from '@/stores/agentStore'
import { useChannelStore } from '@/stores/channelStore'
import { cn } from '@/lib/utils'
import { Bot, User } from 'lucide-react'

interface Props {
  query: string
  selectedIndex: number
  onSelect: (name: string) => void
}

export interface MentionCandidate {
  name: string
  type: 'user' | 'agent'
  displayName?: string
}

export function useMentionCandidates(query: string): MentionCandidate[] {
  const users: { id: string; name: string; displayName?: string }[] = []
  const agents = useAgentStore((s) => s.agents)
  const activeChannelId = useChannelStore((s) => s.activeChannelId)
  const members = useChannelStore((s) => activeChannelId ? s.membersByChannel[activeChannelId] : undefined)

  const memberIds = useMemo(() => {
    if (!members) return null
    return new Set(members.map((m) => m.memberId))
  }, [members])

  const q = query.toLowerCase()

  const userMatches: MentionCandidate[] = users
    .filter((u) => {
      if (memberIds && !memberIds.has(u.id)) return false
      return !q || u.name.toLowerCase().includes(q) || u.displayName?.toLowerCase().includes(q)
    })
    .map((u) => ({ name: u.name, type: 'user', displayName: u.displayName }))

  const agentMatches: MentionCandidate[] = agents
    .filter((a) => {
      if (memberIds && !memberIds.has(a.id)) return false
      return !q || a.name.toLowerCase().includes(q) || a.displayName?.toLowerCase().includes(q)
    })
    .map((a) => ({ name: a.name, type: 'agent', displayName: a.displayName }))

  return [...userMatches, ...agentMatches].slice(0, 8)
}

export function MentionPopup({ query, selectedIndex, onSelect }: Props) {
  const candidates = useMentionCandidates(query)
  const listRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const el = listRef.current?.children[selectedIndex] as HTMLElement | undefined
    el?.scrollIntoView({ block: 'nearest' })
  }, [selectedIndex])

  if (candidates.length === 0) return null

  return (
    <div
      ref={listRef}
      className="absolute bottom-full left-0 mb-1 w-64 max-h-48 overflow-y-auto border bg-popover z-50"
    >
      {candidates.map((c, i) => (
        <button
          key={`${c.type}-${c.name}`}
          onMouseDown={(e) => {
            e.preventDefault()
            onSelect(c.name)
          }}
          className={cn(
            'w-full flex items-center gap-2 px-3 py-2 text-sm text-left transition-colors',
            i === selectedIndex ? 'bg-accent text-accent-foreground' : 'hover:bg-accent/50',
          )}
        >
          {c.type === 'agent' ? (
            <Bot className="h-4 w-4 text-primary shrink-0" />
          ) : (
            <User className="h-4 w-4 text-muted-foreground shrink-0" />
          )}
          <span className="font-medium">@{c.name}</span>
          {c.displayName && c.displayName !== c.name && (
            <span className="text-xs text-muted-foreground truncate">{c.displayName}</span>
          )}
        </button>
      ))}
    </div>
  )
}

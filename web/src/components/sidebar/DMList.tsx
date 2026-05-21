import { useMemo } from 'react'
import { useNavigate } from 'react-router-dom'
import { useChannelStore } from '@/stores/channelStore'
import { useAgentStore } from '@/stores/agentStore'
import { useViewStore } from '@/stores/viewStore'
import { useWorkspacePanelStore } from '@/stores/workspacePanelStore'
import { useSidebarPrefsStore } from '@/stores/sidebarPrefsStore'
import { cn } from '@/lib/utils'
import { MessageCircle, X } from 'lucide-react'
import { CollapsibleSection, ContextMenuTrigger, StatusDot } from '@/components/ui'
import type { MenuEntry } from '@/components/ui'
import type { Channel } from '@/lib/types'
import { agentPath, channelPath } from '@/lib/paths'

export function DMList({ query }: { query?: string }) {
  const navigate = useNavigate()
  const dms = useChannelStore((s) => s.dmChannels)
  const activeId = useChannelStore((s) => s.activeChannelId)
  const setPanel = useWorkspacePanelStore((s) => s.setPanel)
  const agents = useAgentStore((s) => s.agents)
  const hiddenDMIds = useSidebarPrefsStore((s) => s.hiddenDMIds)
  const hideDM = useSidebarPrefsStore((s) => s.hideDM)

  const text = (query ?? '').trim().toLowerCase()
  const visibleAll = useMemo(() => dms.filter((c) => !hiddenDMIds.has(c.id)), [dms, hiddenDMIds])
  const visible = useMemo(
    () => (text ? visibleAll.filter((c) => c.name.toLowerCase().includes(text)) : visibleAll),
    [visibleAll, text],
  )

  const handleDMClick = (ch: Channel) => {
    const agent = agents.find((a) => a.name === ch.name)
    if (agent) {
      useViewStore.getState().setActiveAgent(agent.id)
      useChannelStore.getState().setActiveChannel('')
      navigate(agentPath({ agentId: agent.id }))
    } else {
      setPanel('chat')
      navigate(channelPath({ channelId: ch.id }))
    }
  }

  return (
    <CollapsibleSection
      id="sidebar.dms"
      title="Direct Messages"
      count={visibleAll.length}
    >
      <div className="space-y-0.5">
        {visible.map((ch) => {
          const peerAgent = agents.find((a) => a.name === ch.name)
          const items: MenuEntry[] = [
            { id: 'mark', label: 'Mark all as read', shortcut: '⌥M', onSelect: () => {} },
          ]
          return (
            <ContextMenuTrigger key={ch.id} items={items}>
              <div
                role="button"
                tabIndex={0}
                onClick={() => handleDMClick(ch)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault()
                    handleDMClick(ch)
                  }
                }}
                className={cn(
                  'w-full group flex items-center gap-2 mx-1 px-2 py-1.5 text-sm transition-colors duration-100',
                  activeId === ch.id
                    ? 'bg-primary/10 text-primary font-medium'
                    : 'hover:bg-accent/50 text-foreground/80 hover:text-foreground',
                )}
              >
                {peerAgent ? (
                  <StatusDot status={peerAgent.status as 'online' | 'offline' | 'working' | 'error'} />
                ) : (
                  <MessageCircle
                    className={cn(
                      'h-4 w-4 shrink-0',
                      activeId === ch.id ? 'text-primary' : 'text-muted-foreground',
                    )}
                  />
                )}
                <span className="truncate">{ch.name}</span>
                <span className="ml-auto flex items-center gap-1.5">
                  {ch.unreadCount && ch.unreadCount > 0 ? (
                    <span className="text-[10px] bg-primary text-primary-foreground rounded-full px-1.5 py-0.5 min-w-5 text-center font-semibold">
                      {ch.unreadCount}
                    </span>
                  ) : null}
                  <button
                    type="button"
                    aria-label="Hide DM"
                    onClick={(e) => {
                      e.stopPropagation()
                      hideDM(ch.id)
                    }}
                    className="opacity-0 group-hover:opacity-100 transition-opacity text-muted-foreground hover:text-foreground"
                  >
                    <X className="h-3.5 w-3.5" />
                  </button>
                </span>
              </div>
            </ContextMenuTrigger>
          )
        })}
      </div>
    </CollapsibleSection>
  )
}

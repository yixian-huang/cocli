import { useMemo, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useAgentStore } from '@/stores/agentStore'
import { useChannelStore } from '@/stores/channelStore'
import { useViewStore } from '@/stores/viewStore'
import { useDialogStore } from '@/stores/dialogStore'
import { toast, toastError } from '@/stores/toastStore'
import { cn } from '@/lib/utils'
import {
  StatusDot,
  Badge,
  AttentionBadge,
  CollapsibleSection,
  ContextMenuTrigger,
} from '@/components/ui'
import type { MenuEntry } from '@/components/ui'
import { agentStatusLabel } from '@/lib/status'
import { agentPath } from '@/lib/paths'
import { ChevronDown, ChevronRight, Loader2, RotateCcw } from 'lucide-react'
import { AgentPanel } from './AgentPanel'
import type { Agent } from '@/lib/types'

export function AgentList({ query }: { query?: string }) {
  const navigate = useNavigate()
  const agents = useAgentStore((s) => s.agents)
  const startAgent = useAgentStore((s) => s.startAgent)
  const stopAgent = useAgentStore((s) => s.stopAgent)
  const [expandedId, setExpandedId] = useState<string | null>(null)
  const [restartingId, setRestartingId] = useState<string | null>(null)
  const activeAgentId = useViewStore((s) => s.activeAgentId)
  const dmChannels = useChannelStore((s) => s.dmChannels)
  // Single-tenant: local owner has full access
  const isAdmin = true
  const openCreateAgent = useDialogStore((s) => s.openCreateAgent)

  const text = (query ?? '').trim().toLowerCase()
  const visible = useMemo(
    () => (text ? agents.filter((a) => a.name.toLowerCase().includes(text)) : agents),
    [agents, text],
  )

  const getAgentUnread = (agentName: string): number => {
    const dm = dmChannels.find((c) => c.name === agentName)
    return dm?.unreadCount || 0
  }

  const handleRestart = async (agent: Agent) => {
    if (restartingId) return
    setRestartingId(agent.id)
    try {
      await stopAgent(agent.id)
      await new Promise((resolve) => setTimeout(resolve, 300))
      await startAgent(agent.id)
      toast(`@${agent.name} restarting...`, 'info')
    } catch (err) {
      toastError(err instanceof Error ? err.message : 'Failed to restart agent')
    } finally {
      setRestartingId(null)
    }
  }

  const itemsFor = (): MenuEntry[] =>
    isAdmin
      ? [
          { id: 'rename', label: 'Rename', shortcut: 'F2', onSelect: () => {} },
          '---',
          { id: 'delete', label: 'Delete…', danger: true, onSelect: () => {} },
        ]
      : []

  return (
    <CollapsibleSection
      id="sidebar.agents"
      title="Agents"
      count={agents.length}
      actions={
        <button
          type="button"
          aria-label="New agent"
          onClick={() => openCreateAgent()}
          className="text-primary px-1"
        >
          ＋
        </button>
      }
    >
      <div className="space-y-0.5">
        {visible.map((agent) => {
          const expanded = expandedId === agent.id
          const hasSpecialAttention =
            !!agent.attentionState && agent.attentionState !== 'idle' && agent.attentionState !== 'working'
          return (
            <div key={agent.id}>
              <ContextMenuTrigger items={itemsFor()}>
                <div
                  className={cn(
                    'flex items-center gap-2 w-full px-2 py-1.5 text-sm transition-colors',
                    'border-l-2 border-transparent',
                    activeAgentId === agent.id
                      ? 'bg-surface-raised border-l-accent-signature text-content-primary font-semibold'
                      : 'hover:bg-surface-raised text-content-secondary',
                  )}
                  style={{ transitionDuration: 'var(--motion-fast)', transitionTimingFunction: 'var(--ease-out)' }}
                >
                  <button onClick={() => setExpandedId(expanded ? null : agent.id)} className="shrink-0">
                    {expanded ? (
                      <ChevronDown className="h-3 w-3 text-muted-foreground" />
                    ) : (
                      <ChevronRight className="h-3 w-3 text-muted-foreground" />
                    )}
                  </button>
                  <button
                    onClick={() => navigate(agentPath({ agentId: agent.id }))}
                    className="flex items-center gap-2 flex-1 min-w-0 text-left"
                  >
                    <StatusDot status={agent.status as 'online' | 'offline' | 'working' | 'error'} size="sm" />
                    {agent.status === 'working' && (
                      <Loader2 className="h-3 w-3 shrink-0 -ml-1 animate-spin text-success-emphasis" />
                    )}
                    <span className="font-signal text-content-subtle">@</span>
                    <span className="truncate">{agent.name}</span>
                    {hasSpecialAttention && agent.attentionState && (
                      <AttentionBadge state={agent.attentionState} showIcon={false} className="shrink-0" />
                    )}
                    <span className="ml-auto flex items-center gap-1.5">
                      <span className="text-xs text-muted-foreground truncate max-w-[80px]">
                        {agent.detail || agentStatusLabel(agent.status)}
                      </span>
                      {getAgentUnread(agent.name) > 0 && (
                        <Badge
                          size="sm"
                          variant="default"
                          className="bg-primary text-primary-foreground min-w-5 text-center"
                        >
                          {getAgentUnread(agent.name)}
                        </Badge>
                      )}
                    </span>
                  </button>
                  {agent.attentionState === 'stalled' && (
                    <button
                      onClick={(e) => {
                        e.stopPropagation()
                        void handleRestart(agent)
                      }}
                      disabled={restartingId === agent.id}
                      className="ml-1 inline-flex shrink-0 items-center gap-1 rounded border border-error/25 bg-error/10 px-1.5 py-0.5 text-[11px] text-error-emphasis hover:bg-error/15 disabled:opacity-60"
                      title="通知循环被暂停（点击 restart）"
                    >
                      {restartingId === agent.id ? (
                        <Loader2 className="h-3 w-3 animate-spin" />
                      ) : (
                        <RotateCcw className="h-3 w-3" />
                      )}
                      Restart
                    </button>
                  )}
                </div>
              </ContextMenuTrigger>
              {expanded && <AgentPanel agent={agent} />}
            </div>
          )
        })}
      </div>
    </CollapsibleSection>
  )
}

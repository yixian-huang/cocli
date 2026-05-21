import { useViewStore, type DrawerKey } from '@/stores/viewStore'
import { useAgentStore } from '@/stores/agentStore'
import { ContextBar } from './ContextBar'
import { StatusDot, AttentionBadge } from '@/components/ui'
import { agentStatusLabel } from '@/lib/status'
import {
  ArrowLeft,
  Bot,
  Play,
  Square,
  Loader2,
  Activity,
  Clock,
  Brain,
  Settings as SettingsIcon,
  X,
} from 'lucide-react'
import { cn } from '@/lib/utils'
import { useExitAgentView, useAgentBackLabel } from '@/hooks/useExitAgentView'
import { useTranslation } from 'react-i18next'
import { useMemo } from 'react'

interface AgentHeaderProps {
  agentId: string
  loading: boolean
  onStart: () => void
  onStop: () => void
}

export function AgentHeader({ agentId, loading, onStart, onStop }: AgentHeaderProps) {
  const { t } = useTranslation()
  const drawers = useMemo(
    (): { key: DrawerKey; label: string; Icon: typeof Activity }[] => [
      { key: 'live', label: t('workspace.agent.drawers.live'), Icon: Activity },
      { key: 'history', label: t('workspace.agent.drawers.history'), Icon: Clock },
      { key: 'memory', label: t('workspace.agent.drawers.memory'), Icon: Brain },
    ],
    [t],
  )
  const agent = useAgentStore((s) => s.agents.find((a) => a.id === agentId))
  const subview = useViewStore((s) => s.getSubview(agentId))
  const activeDrawer = useViewStore((s) => s.activeDrawer)
  const toggleDrawer = useViewStore((s) => s.toggleDrawer)
  const setAgentSubview = useViewStore((s) => s.setAgentSubview)
  const exitAgentView = useExitAgentView()
  const backLabel = useAgentBackLabel()

  if (!agent) return null
  const inSettings = subview === 'settings'

  return (
    <div className="h-12 border-b flex items-center px-4 gap-3 shrink-0">
      <button
        onClick={exitAgentView}
        className="p-1 rounded hover:bg-accent text-content-secondary hidden md:flex items-center"
        title={backLabel}
      >
        <ArrowLeft className="h-4 w-4" />
      </button>
      <Bot className="h-4 w-4 text-primary" />
      <span className="font-semibold text-base">@{agent.name}</span>
      <StatusDot
        status={agent.status as 'online' | 'offline' | 'working' | 'error'}
        variant="signature"
      />
      <span className="font-signal text-[10px] uppercase tracking-[0.08em] text-accent-signature ml-1">
        {agentStatusLabel(agent.status)}
      </span>
      {agent.attentionState && agent.attentionState !== 'idle' && (
        <AttentionBadge state={agent.attentionState} />
      )}
      {agent.workingRuntime && (
        <span className="hidden lg:inline text-sm text-content-secondary ml-2 truncate max-w-[280px]">
          {t('workspace.agent.runtimeLine', {
            chat: `${agent.runtime}/${agent.model}`,
            working: `${agent.workingRuntime}/${agent.workingModel}`,
          })}
        </span>
      )}
      {!inSettings && agent.status !== 'offline' && (
        <div className="hidden md:flex ml-2">
          <ContextBar
            lastInputTokens={agent.lastInputTokens}
            contextWindow={agent.contextWindow}
            totalOutputTokens={agent.totalOutputTokens}
            totalCostUSD={agent.totalCostUSD}
            turnCount={agent.turnCount}
            variant="inline"
          />
        </div>
      )}

      <div className="ml-auto flex items-center gap-1">
        {drawers.map(({ key, label, Icon }) => (
          <button
            key={key}
            type="button"
            aria-label={`Open ${label}`}
            disabled={inSettings}
            onClick={() => toggleDrawer(key)}
            className={cn(
              'flex h-7 w-7 items-center justify-center rounded transition-colors',
              activeDrawer === key
                ? 'bg-accent text-foreground'
                : 'text-content-secondary hover:bg-accent hover:text-foreground',
              inSettings && 'opacity-40 pointer-events-none',
            )}
          >
            <Icon className="h-4 w-4" />
          </button>
        ))}
        <button
          type="button"
          aria-label={inSettings ? 'Close settings' : 'Open settings'}
          onClick={() => setAgentSubview(agentId, inSettings ? 'main' : 'settings')}
          className={cn(
            'flex h-7 w-7 items-center justify-center rounded transition-colors',
            inSettings ? 'bg-accent text-foreground' : 'text-content-secondary hover:bg-accent hover:text-foreground',
          )}
        >
          {inSettings ? <X className="h-4 w-4" /> : <SettingsIcon className="h-4 w-4" />}
        </button>
        <span className="w-px h-5 bg-border mx-1" />
        {agent.status === 'offline' || agent.status === 'error' ? (
          <button
            onClick={onStart}
            disabled={loading}
            title="Start agent"
            className="flex h-7 w-7 items-center justify-center text-success-emphasis hover:bg-success/10 disabled:opacity-50"
          >
            {loading ? <Loader2 className="h-4 w-4 animate-spin" /> : <Play className="h-4 w-4" />}
          </button>
        ) : (
          <button
            onClick={onStop}
            disabled={loading}
            title="Stop agent"
            className="flex h-7 w-7 items-center justify-center text-error-emphasis hover:bg-error/10 disabled:opacity-50"
          >
            {loading ? <Loader2 className="h-4 w-4 animate-spin" /> : <Square className="h-4 w-4" />}
          </button>
        )}
      </div>
    </div>
  )
}

import { useState } from 'react'
import { useAgentStore } from '@/stores/agentStore'
import { useViewStore } from '@/stores/viewStore'
import { useKeyboardShortcuts } from '@/hooks/useKeyboardShortcuts'
import { ChatTab } from './ChatTab'
import { WorkspaceTab } from './WorkspaceTab'
import { AgentHeader } from './AgentHeader'
import { AgentDrawer } from './AgentDrawer'
import { AgentSettingsView } from './AgentSettingsView'
import { Tabs } from '@/components/ui'
import { AlertCircle } from 'lucide-react'
import { toast, toastError } from '@/stores/toastStore'

const MAIN_TABS = [
  { key: 'chat', label: 'Chat' },
  { key: 'workspace', label: 'Workspace' },
] as const

type TabKey = (typeof MAIN_TABS)[number]['key']

export function AgentView() {
  const agentId = useViewStore((s) => s.activeAgentId)
  const agent = useAgentStore((s) => s.agents.find((a) => a.id === agentId))
  const subview = useViewStore((s) => (agentId ? s.getSubview(agentId) : 'main'))
  const setAgentSubview = useViewStore((s) => s.setAgentSubview)

  const [tab, setTab] = useState<TabKey>('chat')
  const [loading, setLoading] = useState(false)
  const startAgent = useAgentStore((s) => s.startAgent)
  const stopAgent = useAgentStore((s) => s.stopAgent)

  useKeyboardShortcuts([
    {
      key: 'Escape',
      enabled: subview === 'settings' && !!agentId,
      handler: () => {
        if (agentId) setAgentSubview(agentId, 'main')
      },
    },
  ])

  if (!agentId || !agent) {
    return (
      <div className="flex-1 flex items-center justify-center text-muted-foreground">
        <span className="text-sm">Select an agent</span>
      </div>
    )
  }

  const handleStart = async () => {
    setLoading(true)
    try {
      await startAgent(agent.id)
      toast(`@${agent.name} starting...`, 'success')
    } catch (err) {
      toastError(err instanceof Error ? err.message : 'Failed to start')
    } finally {
      setLoading(false)
    }
  }

  const handleStop = async (force = false) => {
    setLoading(true)
    try {
      await stopAgent(agent.id, force)
      toast(`@${agent.name} stopping...`, 'info')
      await new Promise<void>((resolve, reject) => {
        const current = useAgentStore.getState().agents.find((x) => x.id === agent.id)
        if (current?.status === 'offline') { resolve(); return }
        let finished = false
        const timeoutId = setTimeout(() => {
          cleanup()
          reject(new Error('Stop timed out, agent may still be running'))
        }, 15000)
        const unsub = useAgentStore.subscribe((state) => {
          const a = state.agents.find((x) => x.id === agent.id)
          if (a?.status === 'offline') { cleanup(); resolve() }
        })
        const cleanup = () => {
          if (finished) return
          finished = true
          clearTimeout(timeoutId)
          unsub()
        }
      })
    } catch (err) {
      toastError(err instanceof Error ? err.message : 'Failed to stop')
    } finally {
      setLoading(false)
    }
  }

  const inSettings = subview === 'settings'

  return (
    <div className="flex-1 flex flex-col min-w-0 min-h-0 overflow-hidden relative">
      <AgentHeader
        agentId={agentId}
        loading={loading}
        onStart={handleStart}
        onStop={() => handleStop()}
      />

      {agent.status === 'error' && (
        <div className="flex items-center gap-2 border-b border-error/20 bg-error/10 px-4 py-2 text-xs text-error-emphasis">
          <AlertCircle className="h-3.5 w-3.5 shrink-0" />
          <span className="truncate">{agent.errorDetail || 'Agent encountered an error'}</span>
        </div>
      )}

      {inSettings ? (
        <AgentSettingsView agentId={agentId} />
      ) : (
        <>
          <Tabs
            tabs={MAIN_TABS.map((t) => ({ key: t.key, label: t.label }))}
            active={tab}
            onChange={(key) => setTab(key as TabKey)}
            size="sm"
          />
          <div className="flex-1 min-h-0 flex flex-col overflow-hidden">
            {tab === 'chat' && <ChatTab agentName={agent.name} />}
            {tab === 'workspace' && (
              <WorkspaceTab agentId={agentId} offline={agent.status === 'offline'} />
            )}
          </div>
          <AgentDrawer agentId={agentId} />
        </>
      )}
    </div>
  )
}

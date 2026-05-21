import { useMemo, useState } from 'react'
import { useAgentStore } from '@/stores/agentStore'
import { Tabs } from '@/components/ui'
import { ProfileTab } from './ProfileTab'
import { MemoryTab } from './MemoryTab'
import { OverflowTab } from './OverflowTab'

type SubTab = 'profile' | 'memory' | 'overflow'

export function AgentSettingsView({ agentId }: { agentId: string }) {
  // Single-tenant: local owner has full access
  const isAdmin = true
  const agent = useAgentStore((s) => s.agents.find((a) => a.id === agentId))
  const [tab, setTab] = useState<SubTab>('profile')

  const tabs = useMemo(() => {
    const t: { key: SubTab; label: string }[] = [{ key: 'profile', label: 'Profile' }]
    t.push({ key: 'memory', label: 'Memory' })
    if (isAdmin) t.push({ key: 'overflow', label: 'Overflow' })
    return t
  }, [isAdmin])

  return (
    <div className="flex-1 flex flex-col min-h-0 overflow-hidden">
      <Tabs
        tabs={tabs.map((t) => ({ key: t.key, label: t.label }))}
        active={tab}
        onChange={(k) => setTab(k as SubTab)}
        size="sm"
      />
      <div className="flex-1 min-h-0 flex flex-col overflow-hidden">
        {tab === 'profile' && <ProfileTab agentId={agentId} />}
        {tab === 'memory' && <MemoryTab agentId={agentId} />}
        {tab === 'overflow' && isAdmin && agent && <OverflowTab agent={agent} />}
      </div>
    </div>
  )
}

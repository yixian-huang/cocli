import { useMemo, useState } from 'react'
import { useUserStore } from '@/stores/userStore'
import { useAgentStore } from '@/stores/agentStore'
import { useFeatureFlagStore } from '@/stores/featureFlagStore'
import { Tabs } from '@/components/ui'
import { ProfileTab } from './ProfileTab'
import { SkillsTab } from './SkillsTab'
import { MemoryTab } from './MemoryTab'
import { OverflowTab } from './OverflowTab'

type SubTab = 'profile' | 'skills' | 'memory' | 'overflow'

export function AgentSettingsView({ agentId }: { agentId: string }) {
  const isAdmin = useUserStore((s) => s.user?.role === 'admin')
  const agent = useAgentStore((s) => s.agents.find((a) => a.id === agentId))
  const skillsV2 = useFeatureFlagStore((s) => s.flags['skills_v2'] ?? false)
  const [tab, setTab] = useState<SubTab>('profile')

  const tabs = useMemo(() => {
    const t: { key: SubTab; label: string }[] = [{ key: 'profile', label: 'Profile' }]
    if (skillsV2) t.push({ key: 'skills', label: 'Skills' })
    t.push({ key: 'memory', label: 'Memory' })
    if (isAdmin) t.push({ key: 'overflow', label: 'Overflow' })
    return t
  }, [skillsV2, isAdmin])

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
        {tab === 'skills' && (
          <SkillsTab agentId={agentId} offline={agent?.status === 'offline'} />
        )}
        {tab === 'memory' && <MemoryTab agentId={agentId} />}
        {tab === 'overflow' && isAdmin && agent && <OverflowTab agent={agent} />}
      </div>
    </div>
  )
}

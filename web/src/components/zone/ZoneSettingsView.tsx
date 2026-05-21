import { useState } from 'react'
import { Library } from 'lucide-react'
import { Tabs } from '@/components/ui'
import { SkillsLibraryTab } from './SkillsLibraryTab'

type SubTab = 'library'

const TABS: { key: SubTab; label: string; icon: React.ReactNode }[] = [
  { key: 'library', label: 'Skills Library', icon: <Library className="h-3.5 w-3.5" /> },
]

/**
 * Zone-scoped settings shell. Phase 2 ships only the Skills Library
 * sub-tab; future phases plug in zone members / daemons / billing
 * without restructuring this component.
 *
 * The parent route is responsible for skills_v2 gating — when the flag
 * is off, server returns 404 (Task 21+22) and the route should fall
 * back to a "feature unavailable" page rather than render this view.
 */
export function ZoneSettingsView({ zoneId }: { zoneId: string }) {
  const [tab, setTab] = useState<SubTab>('library')
  return (
    <div className="flex-1 flex flex-col min-h-0 overflow-hidden">
      <Tabs
        tabs={TABS.map((t) => ({ key: t.key, label: t.label, icon: t.icon }))}
        active={tab}
        onChange={(k) => setTab(k as SubTab)}
        size="sm"
      />
      <div className="flex-1 min-h-0 flex flex-col overflow-hidden">
        {tab === 'library' && <SkillsLibraryTab zoneId={zoneId} />}
      </div>
    </div>
  )
}

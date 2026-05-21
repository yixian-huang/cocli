import { useViewStore, type DrawerKey } from '@/stores/viewStore'
import { useKeyboardShortcuts } from '@/hooks/useKeyboardShortcuts'
import { LivePanel } from './panels/LivePanel'
import { HistoryPanel } from './panels/HistoryPanel'
import { MemoryPanel } from './panels/MemoryPanel'
import { X } from 'lucide-react'

const TITLES: Record<DrawerKey, string> = {
  live: 'Live',
  history: 'History',
  memory: 'Memory',
}

export function AgentDrawer({ agentId }: { agentId: string }) {
  const active = useViewStore((s) => s.activeDrawer)
  const setActiveDrawer = useViewStore((s) => s.setActiveDrawer)
  const historySegment = useViewStore((s) => s.historyDrawerSegment)

  useKeyboardShortcuts([
    {
      key: 'Escape',
      enabled: !!active,
      handler: () => setActiveDrawer(null),
    },
  ])

  if (!active) return null

  return (
    <aside
      data-testid={`agent-drawer-${active}`}
      className="absolute right-0 top-0 bottom-0 z-20 w-full md:w-[320px] border-l bg-background shadow-lg flex flex-col"
    >
      <header className="h-10 flex items-center justify-between px-3 border-b shrink-0">
        <span className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
          {TITLES[active]}
        </span>
        <button
          onClick={() => setActiveDrawer(null)}
          aria-label="Close drawer"
          className="p-1 rounded hover:bg-accent text-muted-foreground"
        >
          <X className="h-4 w-4" />
        </button>
      </header>
      <div className="flex-1 min-h-0 overflow-y-auto">
        {active === 'live' && <LivePanel agentId={agentId} />}
        {active === 'history' && (
          <HistoryPanel agentId={agentId} initialSegment={historySegment ?? undefined} />
        )}
        {active === 'memory' && <MemoryPanel agentId={agentId} />}
      </div>
    </aside>
  )
}

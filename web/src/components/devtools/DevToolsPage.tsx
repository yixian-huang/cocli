import { useEffect } from 'react'
import { useDevToolsStore } from '@/stores/devToolsStore'
import { Button } from '@/components/ui'
import { EventStream } from './EventStream'
import { CapacityOverview } from './CapacityOverview'
import { CostSummary } from './CostSummary'
import { cn } from '@/lib/utils'

const EVENT_TYPES = [
  'agent:session',
  'agent:session:end',
  'agent:session:idle',
  'agent:turn',
  'agent:deliver:ack',
  'agent:activity',
  'agent:prompt:info',
  'agent:status',
] as const

export function DevToolsPage() {
  const { subscribe, unsubscribe, isPaused, togglePause, clear } = useDevToolsStore()
  const filters = useDevToolsStore((s) => s.filters)
  const setFilter = useDevToolsStore((s) => s.setFilter)

  useEffect(() => {
    subscribe()
    return () => unsubscribe()
  }, [subscribe, unsubscribe])

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center gap-3 p-4 border-b border-border">
        <h1 className="text-lg font-semibold">DevTools</h1>
      </div>

      <div className="flex-1 overflow-auto">
        <div className="p-4 max-w-5xl mx-auto space-y-6">
          <div className="space-y-3">
            <div className="flex items-center gap-3">
              <h2 className="text-sm font-semibold">Event Stream</h2>
              <Button
                variant={isPaused ? 'primary' : 'secondary'}
                size="sm"
                onClick={togglePause}
              >
                {isPaused ? 'Resume' : 'Pause'}
              </Button>
              <Button variant="ghost" size="sm" onClick={clear}>
                Clear
              </Button>
              <select
                value={filters.eventType ?? ''}
                onChange={(e) => setFilter('eventType', e.target.value || null)}
                className="ml-auto text-[11px] border border-border rounded px-2 py-1 bg-background text-foreground"
              >
                <option value="">All types</option>
                {EVENT_TYPES.map((t) => (
                  <option key={t} value={t}>{t}</option>
                ))}
              </select>
            </div>
            <div className={cn('border rounded-lg overflow-hidden', isPaused && 'opacity-80')}>
              <EventStream />
            </div>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-6 border-t pt-6">
            <div className="space-y-4">
              <h2 className="text-sm font-semibold">Capacity</h2>
              <CapacityOverview />
            </div>
            <div className="space-y-4">
              <h2 className="text-sm font-semibold">Cost</h2>
              <CostSummary />
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

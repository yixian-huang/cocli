import { useState } from 'react'
import { SessionsTab } from '../SessionsTab'
import { ActivityPanel } from '../ActivityPanel'
import { cn } from '@/lib/utils'

type Segment = 'sessions' | 'activity'

interface Props {
  agentId: string
  initialSegment?: Segment
}

export function HistoryPanel({ agentId, initialSegment = 'sessions' }: Props) {
  const [segment, setSegment] = useState<Segment>(initialSegment)

  return (
    <div data-testid="drawer-history" className="flex flex-col h-full">
      <div className="flex border-b shrink-0">
        {(['sessions', 'activity'] as const).map((s) => (
          <button
            key={s}
            type="button"
            data-active={String(segment === s)}
            onClick={() => setSegment(s)}
            className={cn(
              'flex-1 text-xs py-2 font-medium uppercase tracking-wider transition-colors',
              segment === s
                ? 'text-primary border-b-2 border-primary'
                : 'text-muted-foreground hover:text-foreground',
            )}
          >
            {s}
          </button>
        ))}
      </div>
      <div className="flex-1 min-h-0 overflow-hidden flex flex-col">
        {segment === 'sessions' && <SessionsTab agentId={agentId} />}
        {segment === 'activity' && <ActivityPanel agentId={agentId} />}
      </div>
    </div>
  )
}

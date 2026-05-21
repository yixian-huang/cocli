import { useState, useEffect } from 'react'
import { daemons as daemonsApi } from '@/api/client'
import { useZoneStore } from '@/stores/zoneStore'
import { useDialogStore } from '@/stores/dialogStore'
import { Button } from '@/components/ui'
import type { Machine } from '@/lib/types'
import { Plus, Server, Loader2, ChevronDown } from 'lucide-react'
import { VersionStatusBadge } from '@/components/agents/VersionStatusBadge'
import { DaemonDetailPanel } from '@/components/daemons/DaemonDetailPanel'
import { formatLastConnection, formatShortDateTime } from '@/lib/formatTime'
import { cn } from '@/lib/utils'

export function DaemonPanel({
  selectedMachineId,
  onSelectMachineId,
}: {
  selectedMachineId?: string | null
  onSelectMachineId?: (machineId: string) => void
}) {
  const activeZoneId = useZoneStore((s) => s.activeZoneId)
  const openAddDaemon = useDialogStore((s) => s.openAddDaemon)
  const [machines, setMachines] = useState<(Machine & { connected: boolean })[]>([])
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    let active = true
    const fetchDaemons = async () => {
      if (!activeZoneId) return
      try {
        const data = await daemonsApi.list(activeZoneId)
        if (active) setMachines(data)
      } catch {
        // ignore
      } finally {
        if (active) setLoading(false)
      }
    }
    fetchDaemons()
    const interval = setInterval(fetchDaemons, 10000)
    return () => {
      active = false
      clearInterval(interval)
    }
  }, [activeZoneId])

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground flex items-center gap-1.5">
          <Server className="h-3 w-3" /> Daemons
        </h3>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => activeZoneId && openAddDaemon({ zoneId: activeZoneId })}
          className="h-6 px-2 gap-1"
        >
          <Plus className="h-3 w-3" /> Add
        </Button>
      </div>

      {loading ? (
        <div className="flex justify-center py-4">
          <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
        </div>
      ) : machines.length === 0 ? (
        <p className="text-xs text-muted-foreground">No daemons. Click Add to connect one.</p>
      ) : (
        <div className="space-y-2">
          {machines.map((m) => {
            const expanded = selectedMachineId === m.id
            return (
              <div
                key={m.id}
                className={cn(
                  'rounded-lg border overflow-hidden transition-colors',
                  expanded ? 'border-primary/35 shadow-sm' : 'border-border hover:border-border/80',
                )}
              >
                <button
                  type="button"
                  onClick={() => onSelectMachineId?.(m.id)}
                  className={cn(
                    'w-full px-3 py-2.5 text-left transition-colors',
                    expanded ? 'bg-accent/25' : 'hover:bg-accent/15',
                  )}
                  aria-expanded={expanded}
                >
                  <div className="flex items-center gap-2 min-w-0">
                    <span
                      className={cn(
                        'h-2 w-2 rounded-full shrink-0',
                        m.connected ? 'bg-green-500' : 'bg-muted-foreground/40',
                      )}
                      title={m.connected ? 'Online' : 'Offline'}
                    />
                    <span className="font-medium text-sm text-foreground truncate min-w-0">
                      {m.hostname || m.id.slice(0, 12)}
                    </span>
                    {m.daemonVersion ? (
                      <span className="text-[10px] text-muted-foreground shrink-0 font-mono">
                        v{m.daemonVersion}
                      </span>
                    ) : null}
                    <span className="flex-1 min-w-2" />
                    <VersionStatusBadge
                      machineId={m.id}
                      initialStatus={m.versionStatus}
                      initialDaemonVersion={m.daemonVersion}
                      className="shrink-0"
                    />
                    <ChevronDown
                      className={cn(
                        'h-4 w-4 shrink-0 text-muted-foreground transition-transform duration-200',
                        expanded && 'rotate-180',
                      )}
                    />
                  </div>
                  <div className="mt-1.5 pl-4 flex flex-wrap gap-x-4 gap-y-0.5 text-[10px] text-muted-foreground tabular-nums">
                    <span>
                      <span className="text-muted-foreground/70">Created </span>
                      {formatShortDateTime(m.createdAt)}
                    </span>
                    <span>
                      <span className="text-muted-foreground/70">Last seen </span>
                      {formatLastConnection(m.lastSeen, !!m.connected)}
                    </span>
                  </div>
                </button>
                {expanded ? (
                  <DaemonDetailPanel
                    machine={m}
                    onDeleted={() => onSelectMachineId?.(m.id)}
                  />
                ) : null}
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}

// Keep helpers exported for the daemon detail route.

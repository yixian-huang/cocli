import { useCallback } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { DaemonPanel } from '@/components/devtools/DaemonPanel'
import { useZoneStore } from '@/stores/zoneStore'

export function DaemonManagePage() {
  const navigate = useNavigate()
  const { machineId } = useParams<{ machineId?: string }>()
  const zoneSlug = useZoneStore((s) => s.activeZoneSlug)

  const handleSelect = useCallback((id: string) => {
    if (!zoneSlug) return
    const next = machineId === id ? `/z/${zoneSlug}/daemons` : `/z/${zoneSlug}/daemons/${id}`
    navigate(next)
  }, [machineId, navigate, zoneSlug])

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center gap-3 p-4 border-b border-border">
        <h1 className="text-lg font-semibold">Daemon Manage</h1>
      </div>

      <div className="flex-1 overflow-auto">
        <div className="p-4 max-w-5xl mx-auto">
          <DaemonPanel selectedMachineId={machineId ?? null} onSelectMachineId={handleSelect} />
        </div>
      </div>
    </div>
  )
}


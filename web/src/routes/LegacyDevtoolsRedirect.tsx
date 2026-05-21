import { useEffect } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { useZoneStore } from '@/stores/zoneStore'
import { daemonDetailPath, daemonsPath } from '@/lib/paths'

export function LegacyDevtoolsRedirect() {
  const navigate = useNavigate()
  const { machineId } = useParams<{ machineId?: string }>()
  const zoneSlug = useZoneStore((s) => s.activeZoneSlug)

  useEffect(() => {
    if (machineId) {
      navigate(daemonDetailPath({ zoneSlug, machineId }), { replace: true })
      return
    }
    navigate(daemonsPath({ zoneSlug }), { replace: true })
  }, [machineId, navigate, zoneSlug])

  return null
}


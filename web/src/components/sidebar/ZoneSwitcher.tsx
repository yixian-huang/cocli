import { useState, useRef, useEffect } from 'react'
import { useNavigate } from 'react-router-dom'
import { ChevronDown } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { useZoneStore } from '@/stores/zoneStore'
import { useDialogStore } from '@/stores/dialogStore'
import type { Zone } from '@/lib/types'

export function ZoneSwitcher() {
  const navigate = useNavigate()
  const { t } = useTranslation()
  const zones = useZoneStore((s) => s.zones)
  const activeZoneId = useZoneStore((s) => s.activeZoneId)
  const setActiveZone = useZoneStore((s) => s.setActiveZone)
  const openCreateZone = useDialogStore((s) => s.openCreateZone)
  const [open, setOpen] = useState(false)
  const dropdownRef = useRef<HTMLDivElement>(null)

  const activeZone = zones.find((z) => z.id === activeZoneId)

  useEffect(() => {
    function handleClick(e: MouseEvent) {
      if (dropdownRef.current && !dropdownRef.current.contains(e.target as Node)) {
        setOpen(false)
      }
    }
    document.addEventListener('mousedown', handleClick)
    return () => document.removeEventListener('mousedown', handleClick)
  }, [])

  const handleSwitch = (zone: Zone) => {
    setActiveZone(zone.id)
    navigate(`/z/${zone.slug}`)
    setOpen(false)
  }

  if (!activeZone) return null

  return (
    <div className="relative" ref={dropdownRef}>
      <button
        onClick={() => setOpen(!open)}
        className="w-full flex items-center gap-2 px-4 py-2.5 hover:bg-surface-raised transition-colors"
      >
        <div className="w-7 h-7 rounded-md bg-primary flex items-center justify-center text-white text-sm font-bold shrink-0">
          {activeZone.name[0]?.toUpperCase()}
        </div>
        <div className="flex-1 text-left min-w-0">
          <div className="font-signal text-[10px] uppercase tracking-[0.12em] text-content-subtle">
            {t('workspace.sidebar.zoneLabel', 'Zone')}
          </div>
          <div className="text-sm font-bold tracking-tight text-content-primary mt-0.5 truncate">
            {activeZone.name}
          </div>
        </div>
        <ChevronDown className="w-3 h-3 opacity-50 shrink-0" />
      </button>

      {open && (
        <div className="absolute top-full left-0 right-0 z-50 mt-1 mx-2 rounded-lg border border-border bg-popover shadow-lg overflow-hidden">
          {zones.map((zone) => (
            <button
              key={zone.id}
              onClick={() => handleSwitch(zone)}
              className="w-full flex items-center gap-2 px-3 py-2 hover:bg-muted transition-colors"
            >
              <div className="w-6 h-6 rounded-md bg-primary flex items-center justify-center text-white text-xs font-bold">
                {zone.name[0]?.toUpperCase()}
              </div>
              <span className="text-sm flex-1 text-left truncate">{zone.name}</span>
              {zone.id === activeZoneId && (
                <span className="text-primary text-sm">&#10003;</span>
              )}
            </button>
          ))}
          <div className="border-t border-border">
            <button
              onClick={() => {
                setOpen(false)
                openCreateZone()
              }}
              className="w-full flex items-center gap-2 px-3 py-2 hover:bg-muted transition-colors opacity-70"
            >
              <div className="w-6 h-6 rounded-md border-2 border-dashed border-muted-foreground flex items-center justify-center text-lg">
                +
              </div>
              <span className="text-sm">Create Zone</span>
            </button>
          </div>
        </div>
      )}
    </div>
  )
}

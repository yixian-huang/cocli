import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { THEMES } from '@/theme/registry'
import { useZoneStore } from '@/stores/zoneStore'
import { zones as zonesApi } from '@/api/client'
import { toast, toastError } from '@/stores/toastStore'

export function ZoneThemeSelect() {
  const { t } = useTranslation()
  const activeZoneId = useZoneStore((s) => s.activeZoneId)
  const activeZoneThemeId = useZoneStore((s) => s.activeZoneThemeId)
  const [saving, setSaving] = useState(false)

  const onChange = async (themeId: string) => {
    if (!activeZoneId) return
    setSaving(true)
    try {
      await zonesApi.setTheme(activeZoneId, themeId)
      useZoneStore.setState((s) => ({
        ...s,
        activeZoneThemeId: themeId,
        zones: s.zones.map((z) => (z.id === activeZoneId ? { ...z, themeId } : z)),
      }))
      toast(`${t('theme.zoneDefault.title')} ✓`, 'success')
    } catch (e) {
      toastError(e instanceof Error ? e.message : 'Failed to update theme')
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="space-y-1.5">
      <label className="text-[10px] text-content-muted font-medium uppercase tracking-[0.08em]">
        {t('theme.zoneDefault.title')}
      </label>
      <select
        className="w-full bg-surface-panel border border-border-default rounded-[var(--radius-base)] px-2 py-1.5 text-xs text-content-primary"
        value={activeZoneThemeId ?? 'sandstone-light'}
        onChange={(e) => onChange(e.target.value)}
        disabled={saving || !activeZoneId}
      >
        {THEMES.map((th) => (
          <option key={th.id} value={th.id}>
            {t(th.labelKey)}
          </option>
        ))}
      </select>
      <p className="text-[10px] text-content-muted">{t('theme.zoneDefault.description')}</p>
    </div>
  )
}

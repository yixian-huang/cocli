import { useTranslation } from 'react-i18next'
import { useTheme } from '@/theme/useTheme'
import { usePrefsStore } from '@/stores/prefsStore'
import { cn } from '@/lib/utils'

export function ThemeSection() {
  const { t } = useTranslation()
  const { id, themes, setUserTheme } = useTheme()
  const userPref = usePrefsStore((s) => (s.prefs.ui as { theme?: string } | undefined)?.theme)
  const isFollowingZone = userPref === 'follow-zone'

  return (
    <div className="space-y-2">
      <h4 className="text-xs font-semibold">{t('theme.section.title')}</h4>
      <p className="text-[11px] text-content-muted">{t('theme.section.description')}</p>

      <div className="grid grid-cols-3 gap-2">
        {themes.map((theme) => {
          const isActive = !isFollowingZone && id === theme.id
          return (
            <button
              key={theme.id}
              onClick={() => setUserTheme(theme.id)}
              className={cn(
                'flex flex-col items-stretch border rounded-[var(--radius-base)] overflow-hidden transition-colors',
                isActive
                  ? 'border-accent-signature ring-2 ring-accent-signature'
                  : 'border-border-default hover:border-border-strong',
              )}
              type="button"
            >
              <div className="h-12 flex items-stretch">
                <div className="flex-1" style={{ background: theme.preview.bg }} />
                <div className="w-3" style={{ background: theme.preview.accent }} />
              </div>
              <div className="px-2 py-1.5 text-left">
                <div className="text-[11px] font-medium text-content-primary truncate">
                  {t(theme.labelKey)}
                </div>
              </div>
            </button>
          )
        })}
      </div>

      <label className="flex items-center gap-2 text-[11px] text-content-secondary pt-1 cursor-pointer">
        <input
          type="checkbox"
          checked={isFollowingZone}
          onChange={(e) => setUserTheme(e.target.checked ? 'follow-zone' : id)}
          className="accent-accent-signature"
        />
        {t('theme.followZone')}
      </label>
    </div>
  )
}

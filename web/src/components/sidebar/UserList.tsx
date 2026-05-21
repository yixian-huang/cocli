import { useMemo } from 'react'
import { useUserStore } from '@/stores/userStore'
import { usePresenceStore } from '@/stores/presenceStore'
import { useChannelStore } from '@/stores/channelStore'
import { useViewStore } from '@/stores/viewStore'
import { useZoneStore } from '@/stores/zoneStore'
import { dm as dmApi } from '@/api/client'
import { toastError } from '@/stores/toastStore'
import { CollapsibleSection, StatusDot } from '@/components/ui'
import { useTranslation } from 'react-i18next'

export function UserList({ query }: { query?: string }) {
  const { t } = useTranslation()
  const allUsers = useUserStore((s) => s.allUsers)
  const currentUser = useUserStore((s) => s.user)
  const isOnline = usePresenceStore((s) => s.isOnline)
  const setActiveChannel = useChannelStore((s) => s.setActiveChannel)
  const clearActiveAgent = useViewStore((s) => s.clearActiveAgent)

  const sorted = useMemo(() => {
    const text = (query ?? '').trim().toLowerCase()
    const matched = text
      ? allUsers.filter(
          (u) =>
            u.name.toLowerCase().includes(text) ||
            (u.displayName?.toLowerCase().includes(text) ?? false),
        )
      : allUsers
    return [...matched].sort((a, b) => {
      const aOnline = isOnline(a.id) ? 0 : 1
      const bOnline = isOnline(b.id) ? 0 : 1
      if (aOnline !== bOnline) return aOnline - bOnline
      return a.name.localeCompare(b.name)
    })
  }, [allUsers, isOnline, query])

  const handleClick = async (userName: string) => {
    try {
      const zoneId = useZoneStore.getState().activeZoneId
      if (!zoneId) return
      const channel = await dmApi.createOrGet(zoneId, userName)
      clearActiveAgent()
      setActiveChannel(channel.id)
    } catch (err) {
      toastError(err instanceof Error ? err.message : t('sidebar.errors.openDmFailed'))
    }
  }

  return (
    <CollapsibleSection id="sidebar.people" title={t('sidebar.people')} count={allUsers.length}>
      <div className="px-1 pb-2">
        {sorted.map((user) => (
          <button
            key={user.id}
            onClick={() => handleClick(user.name)}
            disabled={user.id === currentUser?.id}
            className="flex items-center gap-2 w-full px-2 py-1 rounded text-sm text-foreground hover:bg-accent transition-colors disabled:opacity-50"
          >
            <StatusDot status={isOnline(user.id) ? 'online' : 'offline'} />
            <span className="truncate">{user.displayName || user.name}</span>
            {user.role === 'admin' && (
              <span className="ml-auto text-[10px] text-muted-foreground">{t('sidebar.admin')}</span>
            )}
          </button>
        ))}
      </div>
    </CollapsibleSection>
  )
}

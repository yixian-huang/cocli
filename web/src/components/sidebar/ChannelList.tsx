import { useMemo } from 'react'
import { useNavigate } from 'react-router-dom'
import { CollapsibleSection, ContextMenuTrigger, Badge } from '@/components/ui'
import type { MenuEntry } from '@/components/ui'
import { useChannelStore } from '@/stores/channelStore'
import { useDialogStore } from '@/stores/dialogStore'
import { useZoneStore } from '@/stores/zoneStore'
import { useUserStore } from '@/stores/userStore'
import { useWorkspacePanelStore } from '@/stores/workspacePanelStore'
import { toast, toastError } from '@/stores/toastStore'
import { channelPath } from '@/lib/paths'
import { cn } from '@/lib/utils'
import { useTranslation } from 'react-i18next'

export function ChannelList({ query }: { query?: string }) {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const channels = useChannelStore((s) => s.channels)
  const archivedChannels = useChannelStore((s) => s.archivedChannels)
  const showArchived = useChannelStore((s) => s.showArchived)
  const toggleShowArchived = useChannelStore((s) => s.toggleShowArchived)
  const setArchived = useChannelStore((s) => s.setArchived)
  const activeId = useChannelStore((s) => s.activeChannelId)
  const isAdmin = useUserStore((s) => s.user?.role === 'admin')
  const zoneId = useZoneStore((s) => s.activeZoneId)
  const zoneSlug = useZoneStore((s) => s.activeZoneSlug)
  const openCreateChannel = useDialogStore((s) => s.openCreateChannel)
  const setPanel = useWorkspacePanelStore((s) => s.setPanel)

  const text = (query ?? '').trim().toLowerCase()
  const visible = useMemo(
    () =>
      text
        ? channels.filter((c) => `${c.displayName || c.name} ${c.name}`.toLowerCase().includes(text))
        : channels,
    [channels, text],
  )
  const visibleArchived = useMemo(
    () =>
      text
        ? archivedChannels.filter((c) => `${c.displayName || c.name} ${c.name}`.toLowerCase().includes(text))
        : archivedChannels,
    [archivedChannels, text],
  )

  const goToChannel = (id: string) => {
    setPanel('chat')
    navigate(channelPath({ zoneSlug, channelId: id }))
  }

  const onArchive = async (id: string, archived: boolean) => {
    try {
      await setArchived(id, archived)
      toast(archived ? t('sidebar.toast.channelArchived') : t('sidebar.toast.channelUnarchived'), 'success')
    } catch (e) {
      toastError(e instanceof Error ? e.message : t('sidebar.errors.archiveFailed'))
    }
  }

  return (
    <CollapsibleSection
      id="sidebar.channels"
      title={t('sidebar.channels')}
      count={channels.length}
      actions={
        <button
          type="button"
          aria-label={t('sidebar.newChannel')}
          onClick={() => zoneId && openCreateChannel({ zoneId })}
          className="text-primary px-1"
        >
          ＋
        </button>
      }
    >
      <ul className="px-1 pb-2">
        {visible.map((c) => {
          const adminItems: MenuEntry[] = isAdmin
            ? [
                { id: 'archive', label: t('sidebar.menu.archive'), onSelect: () => onArchive(c.id, true) },
              ]
            : []
          const items: MenuEntry[] = [
            { id: 'mark', label: t('sidebar.menu.markAllRead'), shortcut: '⌥M', onSelect: () => {} },
            ...adminItems,
          ]
          return (
            <ContextMenuTrigger key={c.id} items={items}>
              <li
                onClick={() => goToChannel(c.id)}
                className={cn(
                  'flex items-center gap-2 w-full px-3 py-1.5 text-sm cursor-pointer transition-colors',
                  'border-l-2 border-transparent',
                  activeId === c.id
                    ? 'bg-surface-raised border-l-accent-signature text-content-primary font-semibold'
                    : 'hover:bg-surface-raised text-content-secondary',
                )}
                style={{ transitionDuration: 'var(--motion-fast)', transitionTimingFunction: 'var(--ease-out)' }}
              >
                <span className="font-signal text-content-subtle">#</span>
                <span className="truncate">{c.displayName || c.name}</span>
                {c.unreadCount && c.unreadCount > 0 ? (
                  <Badge size="sm" className="ml-auto bg-primary text-primary-foreground">
                    {c.unreadCount}
                  </Badge>
                ) : null}
              </li>
            </ContextMenuTrigger>
          )
        })}
      </ul>
      {(archivedChannels.length > 0 || showArchived) && (
        <button
          type="button"
          onClick={toggleShowArchived}
          className="px-3 pb-2 text-[11px] text-muted-foreground hover:text-foreground"
        >
          {showArchived
            ? t('sidebar.hideArchived')
            : t('sidebar.showArchived', { count: archivedChannels.length })}
        </button>
      )}
      {showArchived && (
        <ul className="px-1 pb-2 opacity-60 border-t border-dashed border-border">
          {visibleArchived.map((c) => (
            <ContextMenuTrigger
              key={c.id}
              items={
                isAdmin
                  ? [{ id: 'unarchive', label: t('sidebar.menu.unarchive'), onSelect: () => onArchive(c.id, false) }]
                  : []
              }
            >
              <li
                onClick={() => goToChannel(c.id)}
                className="px-3 py-1 rounded text-sm hover:bg-accent cursor-pointer"
              >
                # {c.displayName || c.name}
              </li>
            </ContextMenuTrigger>
          ))}
        </ul>
      )}
    </CollapsibleSection>
  )
}

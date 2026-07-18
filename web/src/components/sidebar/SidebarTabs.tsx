import { useState, useEffect, useCallback } from 'react'
import { useNavigate } from 'react-router-dom'
import { History, KanbanSquare, KeyRound, ShieldCheck } from 'lucide-react'
import { ChannelList } from './ChannelList'
import { DMList } from './DMList'
import { ThreadInbox } from './ThreadInbox'
import { AgentList } from './AgentList'
import { UserList } from './UserList'
import { SavedMessages } from './SavedMessages'
import { InviteLinks } from './InviteLinks'
import { ZoneSwitcher } from './ZoneSwitcher'
import { useBookmarkStore } from '@/stores/bookmarkStore'
import { useSidebarPrefsStore } from '@/stores/sidebarPrefsStore'
import { useWorkspacePanelStore } from '@/stores/workspacePanelStore'
import { useUserStore } from '@/stores/userStore'
import { useZoneStore } from '@/stores/zoneStore'
import { ListFilter, Tabs } from '@/components/ui'
import { useTranslation } from 'react-i18next'

export function SidebarTabs() {
  const [tab, setTab] = useState<'chat' | 'people'>('chat')
  const [sidebarQuery, setSidebarQuery] = useState('')
  const navigate = useNavigate()
  const { t } = useTranslation()
  const panel = useWorkspacePanelStore((s) => s.panel)
  const setPanel = useWorkspacePanelStore((s) => s.setPanel)
  const activeZoneSlug = useZoneStore((s) => s.activeZoneSlug)
  const isAdmin = useUserStore((s) => s.user?.role === 'admin')
  const fetchBookmarks = useBookmarkStore((s) => s.fetchBookmarks)
  const activeZoneId = useZoneStore((s) => s.activeZoneId)
  useEffect(() => { fetchBookmarks() }, [fetchBookmarks])
  useEffect(() => {
    useSidebarPrefsStore.getState().setZone(activeZoneId || null)
  }, [activeZoneId])

  const openWorkspacePanel = useCallback((next: Parameters<typeof setPanel>[0]) => {
    const slug = activeZoneSlug
    if (!slug) {
      setPanel(next)
      return
    }

    const base = `/z/${slug}`
    const target =
      next === 'history' ? `${base}/history` :
      next === 'zone_tasks' ? `${base}/tasks` :
      next === 'zone_members' ? `${base}/members` :
      next === 'zone_credentials' ? `${base}/keys` :
      base

    navigate(target)
    setPanel(next)
  }, [activeZoneSlug, navigate, setPanel])

  return (
    <div className="flex flex-col h-full">
      {/* Zone switcher */}
      <div className="shrink-0 border-b border-border">
        <ZoneSwitcher />
      </div>

      {/* Tab bar */}
      <div className="shrink-0">
        <Tabs
          tabs={[
            { key: 'chat', label: t('sidebar.tabs.chat') },
            { key: 'people', label: t('sidebar.tabs.people') },
          ]}
          active={tab}
          onChange={(key) => setTab(key as 'chat' | 'people')}
          size="sm"
        />
      </div>

      {/* Global filter */}
      <div className="shrink-0 border-b border-border/60">
        <ListFilter
          value={sidebarQuery}
          onChange={setSidebarQuery}
          placeholder={tab === 'chat' ? t('sidebar.filterChannels') : t('sidebar.filterPeople')}
        />
      </div>

      {/* Tab content */}
      <div className="flex-1 overflow-y-auto overflow-x-hidden">
        {tab === 'chat' ? (
          <>
            <div className="px-2 py-2 border-b border-border/60">
              <button
                onClick={() => openWorkspacePanel('history')}
                className={`w-full flex items-center gap-2 px-2 py-1.5 rounded text-sm ${
                  panel === 'history' ? 'bg-primary/10 text-primary font-medium' : 'hover:bg-accent/50 text-foreground/80'
                }`}
              >
                <History className="w-4 h-4" />
                {t('sidebar.history')}
              </button>
              <button
                onClick={() => openWorkspacePanel('zone_tasks')}
                className={`w-full mt-1 flex items-center gap-2 px-2 py-1.5 rounded text-sm ${
                  panel === 'zone_tasks' ? 'bg-primary/10 text-primary font-medium' : 'hover:bg-accent/50 text-foreground/80'
                }`}
              >
                <KanbanSquare className="w-4 h-4" />
                {t('sidebar.zoneTaskBoard')}
              </button>
            </div>
            <ChannelList query={sidebarQuery} />
            <DMList query={sidebarQuery} />
            <ThreadInbox query={sidebarQuery} />
            <SavedMessages />
          </>
        ) : (
          <>
            <div className="px-2 py-2 border-b border-border/60">
              <button
                onClick={() => openWorkspacePanel('zone_members')}
                className={`w-full flex items-center gap-2 px-2 py-1.5 rounded text-sm ${
                  panel === 'zone_members' ? 'bg-primary/10 text-primary font-medium' : 'hover:bg-accent/50 text-foreground/80'
                }`}
              >
                <ShieldCheck className="w-4 h-4" />
                {t('sidebar.zoneMembers')}
              </button>
              {isAdmin && (
                <>
                  <button
                    onClick={() => openWorkspacePanel('zone_credentials')}
                    className={`w-full mt-1 flex items-center gap-2 px-2 py-1.5 rounded text-sm ${
                      panel === 'zone_credentials' ? 'bg-primary/10 text-primary font-medium' : 'hover:bg-accent/50 text-foreground/80'
                    }`}
                  >
                    <KeyRound className="w-4 h-4" />
                    {t('sidebar.providerKeys')}
                  </button>
                </>
              )}
            </div>
            <UserList query={sidebarQuery} />
            <AgentList query={sidebarQuery} />
            <InviteLinks />
          </>
        )}
      </div>

    </div>
  )
}

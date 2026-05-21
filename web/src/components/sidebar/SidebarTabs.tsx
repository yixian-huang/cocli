import { useState, useEffect } from 'react'
import { useNavigate } from 'react-router-dom'
import { BookOpenText, History, KanbanSquare, Puzzle } from 'lucide-react'
import { ChannelList } from './ChannelList'
import { DMList } from './DMList'
import { ThreadInbox } from './ThreadInbox'
import { AgentList } from './AgentList'
import { SavedMessages } from './SavedMessages'
import { useBookmarkStore } from '@/stores/bookmarkStore'
import { useWorkspacePanelStore } from '@/stores/workspacePanelStore'
import { ListFilter, Tabs } from '@/components/ui'
import { useTranslation } from 'react-i18next'

export function SidebarTabs() {
  const [tab, setTab] = useState<'chat' | 'agents'>('chat')
  const [sidebarQuery, setSidebarQuery] = useState('')
  const navigate = useNavigate()
  const { t } = useTranslation()
  const panel = useWorkspacePanelStore((s) => s.panel)
  const setPanel = useWorkspacePanelStore((s) => s.setPanel)
  // Single-tenant: local owner has full access
  const isAdmin = true
  const fetchBookmarks = useBookmarkStore((s) => s.fetchBookmarks)
  useEffect(() => { fetchBookmarks() }, [fetchBookmarks])

  return (
    <div className="flex flex-col h-full">
      {/* Tab bar */}
      <div className="shrink-0">
        <Tabs
          tabs={[
            { key: 'chat', label: t('sidebar.tabs.chat') },
            { key: 'agents', label: t('sidebar.tabs.people') },
          ]}
          active={tab}
          onChange={(key) => setTab(key as 'chat' | 'agents')}
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
                onClick={() => {
                  navigate('/history')
                  setPanel('history')
                }}
                className={`w-full flex items-center gap-2 px-2 py-1.5 rounded text-sm ${
                  panel === 'history' ? 'bg-primary/10 text-primary font-medium' : 'hover:bg-accent/50 text-foreground/80'
                }`}
              >
                <History className="w-4 h-4" />
                {t('sidebar.history')}
              </button>
              {isAdmin && (
                <button
                  onClick={() => navigate('/tasks')}
                  className="w-full mt-1 flex items-center gap-2 px-2 py-1.5 rounded text-sm hover:bg-accent/50 text-foreground/80"
                >
                  <KanbanSquare className="w-4 h-4" />
                  {t('sidebar.zoneTaskBoard')}
                </button>
              )}
            </div>
            <ChannelList query={sidebarQuery} />
            <DMList query={sidebarQuery} />
            <ThreadInbox query={sidebarQuery} />
            <SavedMessages />
          </>
        ) : (
          <>
            {isAdmin && (
              <div className="px-2 py-2 border-b border-border/60">
                <button
                  onClick={() => navigate('/wiki')}
                  className="w-full flex items-center gap-2 px-2 py-1.5 rounded text-sm hover:bg-accent/50 text-foreground/80"
                >
                  <BookOpenText className="w-4 h-4" />
                  {t('sidebar.wikiAdmin')}
                </button>
              </div>
            )}
            <AgentList query={sidebarQuery} />
          </>
        )}
      </div>

      {/* Footer icons */}
      <div className="shrink-0 border-t border-border px-2 py-1.5 flex items-center justify-end">
        <button
          type="button"
          onClick={() => navigate('/settings/plugins')}
          className="p-1.5 rounded hover:bg-accent text-muted-foreground hover:text-foreground transition-colors"
          title="Plugins"
          aria-label="plugins"
        >
          <Puzzle className="h-4 w-4" />
        </button>
      </div>
    </div>
  )
}

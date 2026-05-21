import { useEffect, useRef, useState, useCallback, useMemo, lazy, Suspense, type ComponentProps } from 'react'
import { Outlet, useLocation } from 'react-router-dom'
import { useChannelStore } from '@/stores/channelStore'
import { useAgentStore } from '@/stores/agentStore'
import { useViewStore } from '@/stores/viewStore'
import { useWorkspacePanelStore } from '@/stores/workspacePanelStore'
import { useWebSocket } from '@/hooks/useWebSocket'
import { SidebarTabs } from '@/components/sidebar/SidebarTabs'
import { BrandLogo } from '@/components/BrandLogo'
import { ChannelHeader } from '@/components/chat/ChannelHeader'
import { MessageList } from '@/components/chat/MessageList'
import { MessageInput } from '@/components/chat/MessageInput'
import { AgentActivity } from '@/components/agents/AgentActivity'
import { AgentView } from '@/components/agents/AgentView'
import { ThreadFocusView } from '@/components/chat/ThreadFocusView'
import { HistoryPanel } from '@/components/history/HistoryPanel'
import { FirstRunWizard } from '@/components/wizard/FirstRunWizard'

const TaskBoard = lazy(() => import('@/components/tasks/TaskBoard').then(m => ({ default: m.TaskBoard })))
const ChannelSettings = lazy(() => import('@/components/chat/ChannelSettings').then(m => ({ default: m.ChannelSettings })))
import { ToastContainer } from '@/components/ui/Toast'
import { ContextMenuPortal } from '@/components/ui/ContextMenu'
import { CreateChannelDialog } from '@/components/sidebar/CreateChannelDialog'
import { OpenDMDialog } from '@/components/sidebar/OpenDMDialog'
import { CreateAgentDialog } from '@/components/agents/CreateAgentDialog'
import { ChannelSwitcher, ShortcutsOverlay } from '@/components/ui'
import { SectionErrorBoundary } from './components/ui/SectionErrorBoundary'
import { Skeleton } from '@/components/ui/Skeleton'
import { useKeyboardShortcuts } from '@/hooks/useKeyboardShortcuts'
import { useExitAgentView, useAgentBackLabel } from '@/hooks/useExitAgentView'
import { useWSStore } from '@/stores/wsStore'
import { useThreadStore } from '@/stores/threadStore'
import { bootstrapPrefs } from '@/stores/prefsStore'
import { ListTodo, Moon, Sun, Menu, X, WifiOff } from 'lucide-react'
import { cn } from '@/lib/utils'
import type { Message } from '@/lib/types'
import { useTranslation } from 'react-i18next'
import { LanguageSwitcher } from '@/components/ui'
import { useTheme } from '@/theme/useTheme'

function AppLayout() {
  const { t } = useTranslation()
  const shortcutSections = useMemo(
    () =>
      [
        {
          title: t('workspace.shortcuts.navigation'),
          items: [
            { keys: ['⌘ K', 'Ctrl K'], description: t('workspace.shortcuts.channelSwitcher') },
            { keys: ['/'], description: t('workspace.shortcuts.focusComposer') },
            { keys: ['J'], description: t('workspace.shortcuts.nextMessage') },
            { keys: ['K'], description: t('workspace.shortcuts.prevMessage') },
            { keys: ['Esc'], description: t('workspace.shortcuts.closeOverlay') },
          ],
        },
        {
          title: t('workspace.shortcuts.workspace'),
          items: [
            { keys: ['?'], description: t('workspace.shortcuts.showHelp') },
            { keys: ['⌘ ⇧ T', 'Ctrl Shift T'], description: t('workspace.shortcuts.toggleTasks') },
            { keys: ['⌘ ⇧ L', 'Ctrl Shift L'], description: t('workspace.shortcuts.toggleDark') },
          ],
        },
      ] satisfies ComponentProps<typeof ShortcutsOverlay>['sections'],
    [t],
  )
  const [showTasks, setShowTasks] = useState(false)
  const [showSettings, setShowSettings] = useState(false)
  const [showChannelSwitcher, setShowChannelSwitcher] = useState(false)
  const [showShortcuts, setShowShortcuts] = useState(false)
  const [sidebarOpen, setSidebarOpen] = useState(false)
  const [searchOpen, setSearchOpen] = useState(false)
  const [searchQuery, setSearchQuery] = useState('')
  const activeId = useChannelStore((s) => s.activeChannelId)
  const { mode, toggleFamilyMode, canToggleFamilyMode } = useTheme()
  const dark = mode === 'dark'
  const toggleDark = toggleFamilyMode
  const location = useLocation()
  const isSettingsRoute = location.pathname.startsWith('/settings/')
  const activeAgentId = useViewStore((s) => s.activeAgentId)
  const clearActiveAgent = useViewStore((s) => s.clearActiveAgent)
  const exitAgentView = useExitAgentView()
  const agentBackLabel = useAgentBackLabel()

  const wsStatus = useWSStore((s) => s.status)
  const workspacePanel = useWorkspacePanelStore((s) => s.panel)
  const setWorkspacePanel = useWorkspacePanelStore((s) => s.setPanel)
  const threadChannel = useThreadStore((s) => s.threadChannel)
  const openThread = useThreadStore((s) => s.openThread)
  const closeThread = useThreadStore((s) => s.closeThread)

  const handleReply = useCallback(
    (message: Message) => {
      if (activeId) openThread(activeId, message)
    },
    [activeId, openThread],
  )

  // Close thread and reset search when switching channels
  useEffect(() => {
    closeThread()
    setSearchOpen(false)
    setSearchQuery('')
    setShowSettings(false)
    setShowChannelSwitcher(false)
    setShowShortcuts(false)
  }, [activeId, closeThread])

  // Fetch channel members when channel changes; clear agent only on actual channel switch
  const prevActiveIdRef = useRef(activeId)
  useEffect(() => {
    if (activeId) {
      if (prevActiveIdRef.current !== activeId) {
        clearActiveAgent()
      }
      prevActiveIdRef.current = activeId
      useChannelStore.getState().fetchMembers(activeId)
    }
  }, [activeId, clearActiveAgent])

  useWebSocket()

  // Close sidebar when channel changes (mobile)
  useEffect(() => {
    setSidebarOpen(false)
  }, [activeId])

  useEffect(() => {
    bootstrapPrefs()
    useChannelStore.getState().fetchChannels()
    useChannelStore.getState().fetchDMs()
    useAgentStore.getState().fetchAgents()
    import('@/stores/threadInboxStore').then(({ useThreadInboxStore }) => {
      useThreadInboxStore.getState().fetchThreads()
    })
  }, [])

  const overlayOpen = showChannelSwitcher || showShortcuts || showSettings || sidebarOpen

  const shortcutDefinitions = useMemo(
    () => [
      {
        key: 'k',
        mod: true,
        shift: false,
        allowInInput: true,
        handler: () => setShowChannelSwitcher((open) => !open),
      },
      {
        key: '?',
        mod: false,
        alt: false,
        handler: () => setShowShortcuts((open) => !open),
      },
      {
        key: '/',
        mod: false,
        shift: false,
        alt: false,
        enabled: !overlayOpen,
        handler: () => {
          const el = document.querySelector<HTMLTextAreaElement>('[data-message-input]')
          el?.focus()
        },
      },
      {
        key: 'j',
        mod: false,
        shift: false,
        alt: false,
        enabled: !overlayOpen && !searchOpen,
        handler: () => {
          window.dispatchEvent(new CustomEvent('message-list:navigate', { detail: { direction: 'next' } }))
        },
      },
      {
        key: 'k',
        mod: false,
        shift: false,
        alt: false,
        enabled: !overlayOpen && !searchOpen,
        handler: () => {
          window.dispatchEvent(new CustomEvent('message-list:navigate', { detail: { direction: 'previous' } }))
        },
      },
      {
        key: 'escape',
        allowInInput: true,
        enabled: showSettings,
        priority: 80,
        handler: () => setShowSettings(false),
      },
      {
        key: 'escape',
        allowInInput: true,
        enabled: sidebarOpen,
        priority: 70,
        handler: () => setSidebarOpen(false),
      },
      {
        key: 't',
        mod: true,
        shift: true,
        allowInInput: true,
        handler: () => setShowTasks((open) => !open),
      },
      {
        key: 'l',
        mod: true,
        shift: true,
        allowInInput: true,
        handler: toggleDark,
      },
    ],
    [overlayOpen, searchOpen, showSettings, sidebarOpen, toggleDark],
  )

  useKeyboardShortcuts(shortcutDefinitions)

  return (
    <>
      <FirstRunWizard />
      <div className="flex h-full w-full overflow-hidden">
      {/* Mobile overlay */}
      {sidebarOpen && (
        <div
          className="fixed inset-0 z-30 bg-foreground/25 md:hidden"
          onClick={() => setSidebarOpen(false)}
        />
      )}

      {/* Sidebar */}
      <aside
        className={cn(
          'w-72 shrink-0 border-r bg-sidebar-bg flex flex-col overflow-hidden text-[15px]',
          'fixed inset-y-0 left-0 z-40 transition-transform duration-200 md:relative md:translate-x-0',
          sidebarOpen ? 'translate-x-0' : '-translate-x-full',
        )}
      >
        <div className="h-14 flex items-center justify-between px-4 border-b border-sidebar-border">
          <BrandLogo textClassName="text-lg" />
          <div className="flex items-center gap-1">
            <button
              onClick={toggleDark}
              disabled={!canToggleFamilyMode}
              className={cn(
                'p-1.5 rounded hover:bg-accent transition-colors text-muted-foreground hover:text-foreground',
                !canToggleFamilyMode && 'opacity-40 cursor-not-allowed',
              )}
              title={
                canToggleFamilyMode
                  ? (dark ? t('workspace.nav.lightMode') : t('workspace.nav.darkMode'))
                  // TODO: i18n in Task 22
                  : 'This theme has no light/dark counterpart yet'
              }
            >
              {dark ? <Sun className="h-3.5 w-3.5" /> : <Moon className="h-3.5 w-3.5" />}
            </button>
            <button
              onClick={() => setSidebarOpen(false)}
              className="p-1.5 rounded hover:bg-accent transition-colors text-muted-foreground hover:text-foreground md:hidden"
            >
              <X className="h-3.5 w-3.5" />
            </button>
          </div>
        </div>
        <div className="flex-1 min-h-0">
          <SectionErrorBoundary name="sidebar">
            <SidebarTabs />
          </SectionErrorBoundary>
        </div>
        <div className="border-t shrink-0">
          <div className="px-3 py-2 flex items-center justify-between gap-2">
            <LanguageSwitcher compact />
          </div>
        </div>
      </aside>

      {/* Main area */}
      <main className="flex-1 flex flex-col min-w-0 min-h-0 overflow-hidden">
        <Outlet />
        {isSettingsRoute ? null : activeAgentId ? (
          <>
            <div className="h-12 border-b flex items-center px-3 gap-2 shrink-0 md:hidden">
              <button
                onClick={() => setSidebarOpen(true)}
                className="p-1.5 rounded hover:bg-accent text-muted-foreground transition-colors"
                title={t('workspace.nav.openSidebar')}
              >
                <Menu className="h-5 w-5" />
              </button>
              <button
                onClick={exitAgentView}
                className="p-1.5 rounded hover:bg-accent text-content-secondary transition-colors text-sm"
                title={agentBackLabel}
              >
                <X className="h-4 w-4" />
              </button>
              <span className="text-sm text-content-secondary">{agentBackLabel}</span>
            </div>
            <SectionErrorBoundary name="agent">
              <AgentView />
            </SectionErrorBoundary>
          </>
        ) : (
          <>
            {workspacePanel !== 'chat' ? (
              <div className="flex-1 min-h-0 flex flex-col">
                <div className="h-12 border-b px-3 flex items-center gap-2 md:hidden">
                  <button
                    onClick={() => setSidebarOpen(true)}
                    className="p-1.5 rounded hover:bg-accent text-muted-foreground transition-colors"
                    title={t('workspace.nav.openSidebar')}
                  >
                    <Menu className="h-5 w-5" />
                  </button>
                  <button
                    onClick={() => setWorkspacePanel('chat')}
                    className="text-sm text-content-secondary hover:text-foreground"
                  >
                    {t('workspace.nav.backToChat')}
                  </button>
                </div>
                {workspacePanel === 'history' && (
                  <SectionErrorBoundary name="history">
                    <HistoryPanel />
                  </SectionErrorBoundary>
                )}
              </div>
            ) : (
              <>
                <div className="flex items-center">
                  <button
                    onClick={() => setSidebarOpen(true)}
                    className="h-12 px-3 border-b flex items-center text-muted-foreground hover:bg-accent transition-colors md:hidden"
                  >
                    <Menu className="h-5 w-5" />
                  </button>
                  <div className="flex-1">
                    <ChannelHeader
                      searchOpen={searchOpen}
                      searchQuery={searchQuery}
                      onSearchToggle={() => {
                        setSearchOpen(!searchOpen)
                        if (searchOpen) setSearchQuery('')
                      }}
                      onSearchChange={setSearchQuery}
                      onSettingsToggle={() => setShowSettings(!showSettings)}
                      settingsOpen={showSettings}
                    />
                  </div>
                  {activeId && (
                    <button
                      onClick={() => setShowTasks(!showTasks)}
                      className={cn(
                        'h-12 px-3 border-b flex items-center gap-1.5 text-sm hover:bg-accent transition-colors',
                        showTasks && 'bg-accent'
                      )}
                    >
                      <ListTodo className="h-4 w-4" />
                      <span className="hidden sm:inline">{t('workspace.nav.tasks')}</span>
                    </button>
                  )}
                </div>
                <div className="flex-1 flex min-h-0">
                  <div className="flex-1 flex flex-col min-w-0">
                    {threadChannel ? (
                      <SectionErrorBoundary name="thread">
                        <ThreadFocusView />
                      </SectionErrorBoundary>
                    ) : (
                      <>
                        <SectionErrorBoundary name="chat">
                          <MessageList onReply={handleReply} searchQuery={searchQuery} />
                        </SectionErrorBoundary>
                        <AgentActivity />
                        <MessageInput />
                      </>
                    )}
                  </div>
                  {showSettings && (
                    <div className="fixed inset-0 z-40 bg-background sm:relative sm:inset-auto sm:z-auto sm:bg-transparent">
                      <Suspense fallback={<Skeleton variant="rectangle" />}>
                        <SectionErrorBoundary name="settings">
                          <ChannelSettings onClose={() => setShowSettings(false)} />
                        </SectionErrorBoundary>
                      </Suspense>
                    </div>
                  )}
                  {showTasks && (
                    <aside className="w-72 border-l overflow-y-auto shrink-0 hidden sm:block">
                      <Suspense fallback={<Skeleton variant="rectangle" />}>
                        <SectionErrorBoundary name="tasks">
                          <TaskBoard />
                        </SectionErrorBoundary>
                      </Suspense>
                    </aside>
                  )}
                </div>
              </>
            )}
          </>
        )}
      </main>

      <ChannelSwitcher open={showChannelSwitcher} onClose={() => setShowChannelSwitcher(false)} />
      <ShortcutsOverlay open={showShortcuts} onClose={() => setShowShortcuts(false)} sections={shortcutSections} />

      {/* WebSocket reconnect banner — suppressed in mock mode (no real backend) */}
      {wsStatus === 'disconnected' && import.meta.env.VITE_USE_MOCK !== 'true' && (
        <div className="fixed bottom-4 left-1/2 -translate-x-1/2 z-50 flex items-center gap-2 rounded-full border border-warning/30 bg-warning/12 px-4 py-2.5 text-sm font-medium text-warning-emphasis shadow-whisper animate-pulse">
          <WifiOff className="h-3.5 w-3.5" />
            {t('ws.reconnecting')}
        </div>
      )}

      <ToastContainer />
      <ContextMenuPortal />
      <CreateChannelDialog />
      <OpenDMDialog />
      <CreateAgentDialog />
    </div>
    </>
  )
}

function App() {
  return <AppLayout />
}

export default App

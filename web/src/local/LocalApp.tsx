import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
} from 'react'
import {
  CheckCircle2,
  Circle,
  CircleDot,
  Download,
  BookOpen,
  History as HistoryIcon,
  Languages,
  ListTodo,
  MessageSquare,
  Moon,
  Network,
  PackageOpen,
  Search,
  Sun,
  Wrench,
  X,
} from 'lucide-react'
import {
  localApi,
  type Agent,
  type AgentStatus,
  type Channel,
  type LiveConnectionState,
  type LiveEvent,
  type Message,
  type RuntimeInfo,
  type GlobalSearchResult,
  type BuiltInWorkspaceProviderKey,
  type Workspace,
  type WorkingState,
  type AgentOperation,
} from './api'
import { LocalSelect } from './LocalSelect'
import { LocalHistoryWorkspace } from './LocalHistoryWorkspace'
import { LocalKnowledgeWorkspace } from './LocalKnowledgeWorkspace'
import { LocalSkillsWorkspace } from './LocalSkillsWorkspace'
import { LocalMcpWorkspace } from './LocalMcpWorkspace'
import { LocalTasksWorkspace } from './LocalTasksWorkspace'
import {
  LANGUAGE_OPTIONS,
  resolveInitialLanguage,
  translate,
  type LocalCopyKey,
  type LocalLanguage,
} from './localization'

type LocalTheme = 'light' | 'dark'
type WorkspaceView =
  | 'chat'
  | 'tasks'
  | 'knowledge'
  | 'agent'
  | 'agent-memory'
  | 'history'
  | 'skills'
  | 'mcp'
  | 'settings'

interface LiveTurn {
  agentId: string
  messageId: string | null
  text: string
  thinking: string
  activity: string[]
  phase: 'running' | 'error' | 'limited'
  canCancel: boolean
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : 'Unexpected local service error'
}

function resolveInitialTheme(): LocalTheme {
  try {
    const stored = localStorage.getItem('cocli-local-theme')
    if (stored === 'light' || stored === 'dark') return stored
  } catch {
    // Storage can be unavailable in privacy-restricted environments.
  }
  return window.matchMedia?.('(prefers-color-scheme: light)').matches ? 'light' : 'dark'
}

export function LocalApp() {
  const [language, setLanguage] = useState<LocalLanguage>(resolveInitialLanguage)
  const [theme, setTheme] = useState<LocalTheme>(resolveInitialTheme)
  const [workspaceView, setWorkspaceView] = useState<WorkspaceView>('chat')
  const [runtimes, setRuntimes] = useState<RuntimeInfo[]>([])
  const [channels, setChannels] = useState<Channel[]>([])
  const [agents, setAgents] = useState<Agent[]>([])
  const [messages, setMessages] = useState<Message[]>([])
  const [channelAgents, setChannelAgents] = useState<Agent[]>([])
  const [channelWorkspaces, setChannelWorkspaces] = useState<Workspace[]>([])
  const [activeAgentId, setActiveAgentId] = useState<string | null>(null)
  const [agentMessages, setAgentMessages] = useState<Message[]>([])
  const [agentChannels, setAgentChannels] = useState<Channel[]>([])
  const [agentWorkspaces, setAgentWorkspaces] = useState<Workspace[]>([])
  const [agentWorkingState, setAgentWorkingState] = useState<WorkingState | null>(null)
  const [agentOperations, setAgentOperations] = useState<AgentOperation[]>([])
  const [agentDraft, setAgentDraft] = useState('')
  const [workspaceKind, setWorkspaceKind] = useState<BuiltInWorkspaceProviderKey>('directory')
  const [workspaceLocator, setWorkspaceLocator] = useState('')
  const [channelWorkspaceKind, setChannelWorkspaceKind] = useState<BuiltInWorkspaceProviderKey>('directory')
  const [channelWorkspaceLocator, setChannelWorkspaceLocator] = useState('')
  const [agentDescription, setAgentDescription] = useState('')
  const [agentInstructions, setAgentInstructions] = useState('')
  const [inviteAgentId, setInviteAgentId] = useState('')
  const [liveTurns, setLiveTurns] = useState<Record<string, LiveTurn>>({})
  const [liveConnection, setLiveConnection] = useState<LiveConnectionState>('connecting')
  const [searchOpen, setSearchOpen] = useState(false)
  const [searchQuery, setSearchQuery] = useState('')
  const [searchResults, setSearchResults] = useState<GlobalSearchResult[]>([])
  const [searchPending, setSearchPending] = useState(false)
  const [searchHasRun, setSearchHasRun] = useState(false)
  const [activeChannelId, setActiveChannelId] = useState<string | null>(null)
  const [pendingMessageId, setPendingMessageId] = useState<string | null>(null)
  const [channelName, setChannelName] = useState('')
  const [channelDescription, setChannelDescription] = useState('')
  const [channelGoal, setChannelGoal] = useState('')
  const [agentName, setAgentName] = useState('')
  const [runtimeName, setRuntimeName] = useState('')
  const [model, setModel] = useState('')
  const [draft, setDraft] = useState('')
  const [loading, setLoading] = useState(true)
  const [messagesLoading, setMessagesLoading] = useState(false)
  const [pending, setPending] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const messageEndRef = useRef<HTMLDivElement>(null)
  const liveTurnCleanupRef = useRef<Record<string, number>>({})
  const t = useCallback(
    (key: LocalCopyKey, values?: Record<string, string | number>) =>
      translate(language, key, values),
    [language],
  )

  const activeChannel = useMemo(
    () => channels.find((channel) => channel.id === activeChannelId) ?? null,
    [activeChannelId, channels],
  )
  const activeAgent = useMemo(
    () => agents.find((agent) => agent.id === activeAgentId) ?? null,
    [activeAgentId, agents],
  )
  const availableChannelAgents = useMemo(
    () => agents.filter((agent) => !channelAgents.some((member) => member.id === agent.id)),
    [agents, channelAgents],
  )
  const installedRuntimes = useMemo(
    () => runtimes.filter((runtime) => runtime.installed),
    [runtimes],
  )
  const selectedRuntime = useMemo(
    () => runtimes.find((runtime) => runtime.name === runtimeName) ?? null,
    [runtimeName, runtimes],
  )
  const agentNames = useMemo(
    () => new Map(agents.map((agent) => [agent.id, agent.name])),
    [agents],
  )
  const runtimeOptions = useMemo(
    () => installedRuntimes.map((runtime) => ({
      value: runtime.name,
      label: runtime.name,
      meta: runtime.version ?? undefined,
    })),
    [installedRuntimes],
  )
  const modelOptions = useMemo(
    () => selectedRuntime?.models.map((runtimeModel) => ({
      value: runtimeModel,
      label: runtimeModel,
    })) ?? [],
    [selectedRuntime],
  )

  useEffect(() => {
    const root = document.documentElement
    root.dataset.localTheme = theme
    root.style.colorScheme = theme
    try {
      localStorage.setItem('cocli-local-theme', theme)
    } catch {
      // Keep the in-memory preference when persistence is unavailable.
    }
  }, [theme])

  useEffect(() => {
    document.documentElement.lang = language
    try {
      localStorage.setItem('cocli-local-language', language)
    } catch {
      // Keep the in-memory preference when persistence is unavailable.
    }
  }, [language])

  useEffect(() => {
    let cancelled = false
    async function bootstrap() {
      try {
        const [nextRuntimes, nextChannels, nextAgents] = await Promise.all([
          localApi.listRuntimes(),
          localApi.listChannels(),
          localApi.listAgents(),
        ])
        if (cancelled) return
        setRuntimes(nextRuntimes)
        setChannels(nextChannels)
        setAgents(nextAgents)
        setActiveChannelId((current) => current ?? nextChannels[0]?.id ?? null)
        setActiveAgentId((current) => current ?? nextAgents[0]?.id ?? null)
        const firstRuntime = nextRuntimes.find((runtime) => runtime.installed)
        setRuntimeName((current) => current || firstRuntime?.name || '')
        setModel((current) => current || firstRuntime?.models[0] || '')
      } catch (nextError) {
        if (!cancelled) setError(errorMessage(nextError))
      } finally {
        if (!cancelled) setLoading(false)
      }
    }
    void bootstrap()
    return () => {
      cancelled = true
    }
  }, [])

  useEffect(() => {
    if (!activeChannelId) {
      setChannelAgents([])
      setChannelWorkspaces([])
      return
    }
    let cancelled = false
    void Promise.all([
      localApi.listChannelMembers(activeChannelId),
      localApi.listChannelWorkspaces(activeChannelId),
    ])
      .then(([members, workspaces]) => {
        if (!cancelled) {
          setChannelAgents(members)
          setChannelWorkspaces(workspaces)
        }
      })
      .catch((nextError) => {
        if (!cancelled) setError(errorMessage(nextError))
      })
    return () => {
      cancelled = true
    }
  }, [activeChannelId, agents])

  useEffect(() => {
    if (!activeAgentId) {
      setAgentMessages([])
      setAgentChannels([])
      setAgentWorkspaces([])
      setAgentWorkingState(null)
      setAgentOperations([])
      return
    }
    let cancelled = false
    let refreshing = false
    const refreshAgent = async () => {
      if (refreshing) return
      refreshing = true
      try {
        const [nextMessages, nextChannels, nextWorkspaces, nextWorkingState, nextOperations] =
          await Promise.all([
            localApi.listAgentMessages(activeAgentId),
            localApi.listAgentChannels(activeAgentId),
            localApi.listAgentWorkspaces(activeAgentId),
            localApi.getAgentWorkingState(activeAgentId),
            localApi.listAgentOperations(activeAgentId),
          ])
        if (cancelled) return
        setAgentMessages(nextMessages)
        setAgentChannels(nextChannels.filter((channel) => !channel.is_system))
        setAgentWorkspaces(nextWorkspaces)
        setAgentWorkingState(nextWorkingState)
        setAgentOperations(nextOperations)
      } catch (nextError) {
        if (!cancelled) setError(errorMessage(nextError))
      } finally {
        refreshing = false
      }
    }
    void refreshAgent()
    const interval = window.setInterval(() => void refreshAgent(), 2_000)
    return () => {
      cancelled = true
      window.clearInterval(interval)
    }
  }, [activeAgentId])

  useEffect(() => {
    if (!activeChannelId) {
      setMessages([])
      return
    }
    let cancelled = false
    let refreshing = false
    const refreshMessages = async (showLoading: boolean) => {
      if (refreshing) return
      refreshing = true
      if (showLoading) setMessagesLoading(true)
      try {
        const nextMessages = await localApi.listMessages(activeChannelId)
        if (!cancelled) setMessages(nextMessages)
      } catch (nextError: unknown) {
        if (!cancelled) setError(errorMessage(nextError))
      } finally {
        refreshing = false
        if (!cancelled && showLoading) setMessagesLoading(false)
      }
    }
    void refreshMessages(true)
    setLiveTurns({})
    const unsubscribe = localApi.subscribeToEvents(
      (event: LiveEvent) => {
        if (event.channelId !== activeChannelId) return
        if (event.kind === 'delivery_completed') {
          if (event.agentId) {
            const timer = liveTurnCleanupRef.current[event.agentId]
            if (timer) window.clearTimeout(timer)
            delete liveTurnCleanupRef.current[event.agentId]
            setLiveTurns((current) => {
              const next = { ...current }
              delete next[event.agentId as string]
              return next
            })
          }
          void refreshMessages(false)
          return
        }
        if (!event.agentId) return
        if (event.kind === 'turn_finished') {
          const agentId = event.agentId
          const messageId = event.messageId
          const existingTimer = liveTurnCleanupRef.current[agentId]
          if (existingTimer) window.clearTimeout(existingTimer)
          liveTurnCleanupRef.current[agentId] = window.setTimeout(() => {
            delete liveTurnCleanupRef.current[agentId]
            setLiveTurns((current) => {
              if (current[agentId]?.messageId !== messageId) return current
              const next = { ...current }
              delete next[agentId]
              return next
            })
          }, 4_000)
        }
        if (event.kind === 'turn_started') {
          const timer = liveTurnCleanupRef.current[event.agentId]
          if (timer) window.clearTimeout(timer)
          delete liveTurnCleanupRef.current[event.agentId]
          void localApi.getRuntimeStatus(event.agentId).then((runtimeStatus) => {
            setLiveTurns((current) => {
              const turn = current[event.agentId as string]
              if (!turn) return current
              return {
                ...current,
                [event.agentId as string]: {
                  ...turn,
                  canCancel: runtimeStatus.supports_turn_cancel,
                },
              }
            })
          }).catch(() => {
            // Live controls are optional; execution visibility remains available.
          })
        }
        setLiveTurns((current) => {
          const previous = current[event.agentId as string] ?? {
            agentId: event.agentId as string,
            messageId: event.messageId,
            text: '',
            thinking: '',
            activity: [],
            phase: 'running' as const,
            canCancel: false,
          }
          const next: LiveTurn = { ...previous, messageId: event.messageId ?? previous.messageId }
          const text = typeof event.payload.text === 'string' ? event.payload.text : ''
          if (event.kind === 'turn_started') {
            next.text = ''
            next.thinking = ''
            next.activity = []
            next.phase = 'running'
          } else if (event.kind === 'thinking_delta') {
            next.thinking = `${next.thinking}${text}`.slice(-240)
          } else if (event.kind === 'text_delta') {
            next.text += text
          } else if (event.kind === 'tool_started') {
            const name = typeof event.payload.name === 'string' ? event.payload.name : t('liveTool')
            next.activity = [...next.activity, name].slice(-4)
          } else if (event.kind === 'turn_error') {
            next.phase = 'error'
          } else if (event.kind === 'rate_limited') {
            next.phase = 'limited'
          } else if (event.kind === 'turn_finished') {
            const status = typeof event.payload.status === 'string' ? event.payload.status : ''
            if (status === 'Failed' || status === 'Cancelled') next.phase = 'error'
          }
          return { ...current, [event.agentId as string]: next }
        })
      },
      setLiveConnection,
    )
    const interval = window.setInterval(() => {
      void refreshMessages(false)
    }, 2_000)
    return () => {
      cancelled = true
      unsubscribe()
      Object.values(liveTurnCleanupRef.current).forEach((timer) => window.clearTimeout(timer))
      liveTurnCleanupRef.current = {}
      window.clearInterval(interval)
    }
  }, [activeChannelId, t])

  useEffect(() => {
    messageEndRef.current?.scrollIntoView({ behavior: 'smooth', block: 'end' })
  }, [messages])

  useEffect(() => {
    if (workspaceView !== 'chat' || !pendingMessageId) return
    const target = document.getElementById(`local-message-${pendingMessageId}`)
    if (!target) return
    target.scrollIntoView({ behavior: 'smooth', block: 'center' })
    const timer = window.setTimeout(() => setPendingMessageId(null), 1_600)
    return () => window.clearTimeout(timer)
  }, [messages, pendingMessageId, workspaceView])

  function formatTime(value: string): string {
    return new Intl.DateTimeFormat(language, {
      hour: '2-digit',
      minute: '2-digit',
    }).format(new Date(value))
  }

  async function createChannel(event: FormEvent) {
    event.preventDefault()
    const name = channelName.trim()
    if (!name) return
    setPending('channel')
    setError(null)
    try {
      const channel = await localApi.createChannel({
        name,
        description: channelDescription.trim() || undefined,
        goal: channelGoal.trim() || undefined,
      })
      setChannels((current) => [...current, channel])
      setActiveChannelId(channel.id)
      setChannelName('')
      setChannelDescription('')
      setChannelGoal('')
    } catch (nextError) {
      setError(errorMessage(nextError))
    } finally {
      setPending(null)
    }
  }

  function selectRuntime(name: string) {
    setRuntimeName(name)
    const runtime = runtimes.find((candidate) => candidate.name === name)
    setModel(runtime?.models[0] ?? '')
  }

  async function createAgent(event: FormEvent) {
    event.preventDefault()
    if (!agentName.trim() || !runtimeName) return
    setPending('agent')
    setError(null)
    try {
      const agent = await localApi.createAgent({
        ...(workspaceView === 'chat' && activeChannelId ? { channel_id: activeChannelId } : {}),
        name: agentName.trim(),
        description: agentDescription.trim() || undefined,
        instructions: agentInstructions.trim() || undefined,
        runtime: runtimeName,
        model: model || null,
      })
      setAgents((current) => [...current, agent])
      if (workspaceView === 'chat' && activeChannelId) {
        setChannelAgents((current) => (
          current.some((member) => member.id === agent.id) ? current : [...current, agent]
        ))
      }
      setActiveAgentId(agent.id)
      setAgentName('')
      setAgentDescription('')
      setAgentInstructions('')
    } catch (nextError) {
      setError(errorMessage(nextError))
    } finally {
      setPending(null)
    }
  }

  async function joinAgentToActiveChannel() {
    if (!activeChannelId || !inviteAgentId) return
    setPending('join-agent')
    setError(null)
    try {
      await localApi.addChannelMember(activeChannelId, {
        agent_id: inviteAgentId,
        delivery_policy: 'subscribed',
      })
      const members = await localApi.listChannelMembers(activeChannelId)
      setChannelAgents(members)
      setInviteAgentId('')
    } catch (nextError) {
      setError(errorMessage(nextError))
    } finally {
      setPending(null)
    }
  }

  async function setAgentStatus(agentId: string, status: AgentStatus) {
    setPending(agentId)
    setError(null)
    try {
      const updated = await localApi.setAgentStatus(agentId, status)
      setAgents((current) => current.map((agent) => agent.id === updated.id ? updated : agent))
      setChannelAgents((current) => current.map((agent) => (
        agent.id === updated.id ? updated : agent
      )))
    } catch (nextError) {
      setError(errorMessage(nextError))
    } finally {
      setPending(null)
    }
  }

  async function sendMessage(event: FormEvent) {
    event.preventDefault()
    const content = draft.trim()
    if (!activeChannelId || !content) return
    setPending('message')
    setError(null)
    try {
      const response = await localApi.postMessage(activeChannelId, content)
      setMessages((current) =>
        [...current, response.message, ...response.replies].sort((left, right) => left.seq - right.seq),
      )
      setDraft('')
    } catch (nextError) {
      setError(errorMessage(nextError))
    } finally {
      setPending(null)
    }
  }

  async function sendAgentMessage(event: FormEvent) {
    event.preventDefault()
    const content = agentDraft.trim()
    if (!activeAgentId || !content) return
    setPending('agent-message')
    setError(null)
    try {
      const response = await localApi.postAgentMessage(activeAgentId, content)
      setAgentMessages((current) =>
        [...current, response.message, ...response.replies]
          .sort((left, right) => left.seq - right.seq),
      )
      setAgentDraft('')
    } catch (nextError) {
      setError(errorMessage(nextError))
    } finally {
      setPending(null)
    }
  }

  async function attachAgentWorkspace(event: FormEvent) {
    event.preventDefault()
    const locator = workspaceLocator.trim()
    if (!activeAgentId || !locator) return
    setPending('agent-workspace')
    setError(null)
    try {
      const workspace = await localApi.attachAgentWorkspace(activeAgentId, {
        kind: workspaceKind,
        locator,
      })
      setAgentWorkspaces((current) => [...current, workspace])
      setWorkspaceLocator('')
    } catch (nextError) {
      setError(errorMessage(nextError))
    } finally {
      setPending(null)
    }
  }

  async function attachChannelWorkspace(event: FormEvent) {
    event.preventDefault()
    const locator = channelWorkspaceLocator.trim()
    if (!activeChannelId || !locator) return
    setPending('channel-workspace')
    setError(null)
    try {
      const workspace = await localApi.attachChannelWorkspace(activeChannelId, {
        kind: channelWorkspaceKind,
        locator,
      })
      setChannelWorkspaces((current) => [...current, workspace])
      setChannelWorkspaceLocator('')
    } catch (nextError) {
      setError(errorMessage(nextError))
    } finally {
      setPending(null)
    }
  }

  async function cancelLiveTurn(agentId: string) {
    setPending(`cancel-${agentId}`)
    setError(null)
    try {
      await localApi.cancelTurn(agentId)
    } catch (nextError) {
      setError(errorMessage(nextError))
    } finally {
      setPending(null)
    }
  }

  async function runGlobalSearch(event: FormEvent) {
    event.preventDefault()
    const query = searchQuery.trim()
    if (!query) return
    setSearchPending(true)
    setError(null)
    try {
      const response = await localApi.globalSearch(query)
      setSearchResults(response.results)
      setSearchHasRun(true)
    } catch (nextError) {
      setError(errorMessage(nextError))
    } finally {
      setSearchPending(false)
    }
  }

  function openSearchResult(result: GlobalSearchResult) {
    if (result.channelId) setActiveChannelId(result.channelId)
    if (result.kind === 'message') {
      if (result.agentId) {
        setActiveAgentId(result.agentId)
        setWorkspaceView('agent')
      } else {
        if (result.messageId) setPendingMessageId(result.messageId)
        setWorkspaceView('chat')
      }
    } else if (result.kind === 'task') {
      setWorkspaceView('tasks')
    } else if (result.kind === 'agent') {
      setActiveAgentId(result.id)
      setWorkspaceView('agent')
    } else if (result.kind === 'channel') {
      setWorkspaceView('chat')
    } else {
      setWorkspaceView('knowledge')
    }
    setSearchOpen(false)
  }

  if (loading) {
    return <main className="local-loading">{t('loading')}</main>
  }

  const agentCountLabel = t(
    channelAgents.length === 1 ? 'agentCount' : 'agentCountPlural',
    { count: channelAgents.length },
  )
  const onboardingSteps = [
    { done: installedRuntimes.length > 0, label: t('onboardingRuntime') },
    { done: Boolean(activeChannel), label: t('onboardingChannel') },
    { done: channelAgents.length > 0, label: t('onboardingAgent') },
    { done: messages.some((message) => message.role === 'user'), label: t('onboardingTask') },
  ]
  const showOnboarding = !onboardingSteps.every((step) => step.done)
  const primarySection: 'channels' | 'agents' | 'settings' =
    workspaceView === 'settings'
      ? 'settings'
      : workspaceView === 'agent'
          || workspaceView === 'agent-memory'
          || workspaceView === 'history'
          || workspaceView === 'skills'
          || workspaceView === 'mcp'
        ? 'agents'
        : 'channels'

  return (
    <main className="local-shell">
      <header className="local-topbar">
        <div className="brand-lockup">
          <span className="brand-mark" aria-hidden="true">c</span>
          <strong>{t('brand')}</strong>
        </div>

        <nav className="local-primary-nav" aria-label={t('workspaceNavigation')}>
          <button
            type="button"
            className={primarySection === 'channels' ? 'active' : ''}
            aria-current={primarySection === 'channels' ? 'page' : undefined}
            onClick={() => setWorkspaceView('chat')}
          >
            <MessageSquare size={14} aria-hidden="true" />
            {t('channels')}
          </button>
          <button
            type="button"
            className={primarySection === 'agents' ? 'active' : ''}
            aria-current={primarySection === 'agents' ? 'page' : undefined}
            onClick={() => setWorkspaceView('agent')}
          >
            <CircleDot size={14} aria-hidden="true" />
            {t('agents')}
          </button>
          <button
            type="button"
            className={primarySection === 'settings' ? 'active' : ''}
            aria-current={primarySection === 'settings' ? 'page' : undefined}
            onClick={() => setWorkspaceView('settings')}
          >
            <Wrench size={14} aria-hidden="true" />
            {t('settings')}
          </button>
        </nav>

        <div className="topbar-actions">
          <button
            type="button"
            className="global-search-trigger"
            aria-label={t('globalSearch')}
            onClick={() => setSearchOpen(true)}
          >
            <Search size={15} aria-hidden="true" />
            <span>{t('search')}</span>
          </button>
          <a
            className="backup-trigger"
            href="/api/backups/state"
            download
            aria-label={t('downloadBackup')}
            title={t('downloadBackup')}
          >
            <Download size={15} aria-hidden="true" />
          </a>
          <div className="service-status">
            <span className={`status-dot ${liveConnection}`} aria-hidden="true" />
            {liveConnection === 'connected'
              ? t('liveConnected')
              : liveConnection === 'unavailable' ? t('serviceOnline') : t('liveReconnecting')}
          </div>
          <div className="preference-divider" aria-hidden="true" />
          <div className="language-control">
            <Languages size={15} strokeWidth={1.8} aria-hidden="true" />
            <LocalSelect
              id="local-language"
              ariaLabel={t('language')}
              value={language}
              options={LANGUAGE_OPTIONS}
              onChange={(value) => setLanguage(value as LocalLanguage)}
              placeholder={t('language')}
              compact
            />
          </div>
          <button
            type="button"
            className="theme-toggle"
            aria-label={`${t('appearance')}: ${theme === 'dark' ? t('darkMode') : t('lightMode')}`}
            onClick={() => setTheme((current) => current === 'dark' ? 'light' : 'dark')}
          >
            {theme === 'dark'
              ? <Moon size={15} strokeWidth={1.8} aria-hidden="true" />
              : <Sun size={15} strokeWidth={1.8} aria-hidden="true" />}
            <span>{theme === 'dark' ? t('darkMode') : t('lightMode')}</span>
          </button>
        </div>
      </header>

      {primarySection === 'channels' && (
        <nav className="local-secondary-nav" aria-label={t('channelWorkspace')}>
          <button
            type="button"
            className={workspaceView === 'chat' ? 'active' : ''}
            onClick={() => setWorkspaceView('chat')}
          >
            <MessageSquare size={14} aria-hidden="true" />
            {t('conversation')}
          </button>
          <button
            type="button"
            className={workspaceView === 'tasks' ? 'active' : ''}
            onClick={() => setWorkspaceView('tasks')}
          >
            <ListTodo size={14} aria-hidden="true" />
            {t('tasksWorkspace')}
          </button>
          <button
            type="button"
            className={workspaceView === 'knowledge' ? 'active' : ''}
            onClick={() => setWorkspaceView('knowledge')}
          >
            <BookOpen size={14} aria-hidden="true" />
            {t('knowledgeMemory')}
          </button>
        </nav>
      )}

      {primarySection === 'agents' && (
        <nav className="local-secondary-nav" aria-label={t('agentWorkspace')}>
          <button
            type="button"
            className={workspaceView === 'agent' ? 'active' : ''}
            onClick={() => setWorkspaceView('agent')}
          >
            <MessageSquare size={14} aria-hidden="true" />
            {t('conversation')}
          </button>
          <button
            type="button"
            className={workspaceView === 'agent-memory' ? 'active' : ''}
            onClick={() => setWorkspaceView('agent-memory')}
          >
            <BookOpen size={14} aria-hidden="true" />
            {t('knowledgeMemory')}
          </button>
          <button
            type="button"
            className={workspaceView === 'history' ? 'active' : ''}
            onClick={() => setWorkspaceView('history')}
          >
            <HistoryIcon size={14} aria-hidden="true" />
            {t('historyWorkspace')}
          </button>
          <button
            type="button"
            className={workspaceView === 'skills' ? 'active' : ''}
            onClick={() => setWorkspaceView('skills')}
          >
            <PackageOpen size={14} aria-hidden="true" />
            {t('skillsWorkspace')}
          </button>
          <button
            type="button"
            className={workspaceView === 'mcp' ? 'active' : ''}
            onClick={() => setWorkspaceView('mcp')}
          >
            <Network size={14} aria-hidden="true" />
            {t('mcpWorkspace')}
          </button>
        </nav>
      )}

      {searchOpen && (
        <div className="search-overlay" role="presentation">
          <section className="global-search" role="dialog" aria-modal="true" aria-labelledby="global-search-title">
            <header>
              <div>
                <span className="eyebrow">{t('globalSearchEyebrow')}</span>
                <h2 id="global-search-title">{t('globalSearch')}</h2>
              </div>
              <button type="button" aria-label={t('close')} onClick={() => setSearchOpen(false)}>
                <X size={16} aria-hidden="true" />
              </button>
            </header>
            <form onSubmit={runGlobalSearch}>
              <Search size={16} aria-hidden="true" />
              <input
                autoFocus
                aria-label={t('globalSearch')}
                value={searchQuery}
                onChange={(event) => {
                  setSearchQuery(event.target.value)
                  setSearchResults([])
                  setSearchHasRun(false)
                }}
                placeholder={t('globalSearchPlaceholder')}
              />
              <button type="submit" disabled={!searchQuery.trim() || searchPending}>
                {searchPending ? t('searching') : t('search')}
              </button>
            </form>
            <div className="global-search-results" aria-live="polite">
              {searchResults.map((result) => (
                <button type="button" key={`${result.kind}-${result.id}`} onClick={() => openSearchResult(result)}>
                  <span>{result.kind}</span>
                  <strong>{result.title}</strong>
                  <small>{result.snippet}</small>
                </button>
              ))}
              {!searchPending && searchHasRun && searchResults.length === 0 && (
                <p>{t('globalSearchEmpty')}</p>
              )}
            </div>
          </section>
        </div>
      )}

      {error && (
        <div className="error-banner" role="alert">
          <span>{error}</span>
          <button type="button" onClick={() => setError(null)}>{t('dismiss')}</button>
        </div>
      )}

      {workspaceView === 'chat' ? (
      <div className="local-grid">
        <aside className="channel-rail" aria-label={t('channelsAndRuntimes')}>
          <section>
            <div className="section-heading">
              <h2>{t('channels')}</h2>
              <span>{channels.length}</span>
            </div>
            <nav className="channel-list" aria-label={t('channelsNav')}>
              {channels.map((channel) => (
                <button
                  key={channel.id}
                  type="button"
                  className={channel.id === activeChannelId ? 'active' : ''}
                  onClick={() => setActiveChannelId(channel.id)}
                >
                  <span aria-hidden="true">#</span>
                  {channel.name}
                </button>
              ))}
              {channels.length === 0 && <p className="quiet-copy">{t('noChannels')}</p>}
            </nav>
            <form className="inline-create" onSubmit={createChannel}>
              <label htmlFor="channel-name">{t('newChannel')}</label>
              <input
                id="channel-name"
                value={channelName}
                onChange={(event) => setChannelName(event.target.value)}
                placeholder={t('channelPlaceholder')}
                autoComplete="off"
              />
              <label htmlFor="channel-description">{t('description')}</label>
              <input
                id="channel-description"
                value={channelDescription}
                onChange={(event) => setChannelDescription(event.target.value)}
                placeholder={t('channelDescriptionPlaceholder')}
              />
              <label htmlFor="channel-goal">{t('channelGoal')}</label>
              <textarea
                id="channel-goal"
                value={channelGoal}
                onChange={(event) => setChannelGoal(event.target.value)}
                placeholder={t('channelGoalPlaceholder')}
                rows={2}
              />
              <button type="submit" disabled={!channelName.trim() || pending === 'channel'}>
                {t('add')}
              </button>
            </form>
          </section>

          <section className="runtime-section">
            <div className="section-heading">
              <h2>{t('runtimes')}</h2>
              <span>{installedRuntimes.length}/{runtimes.length}</span>
            </div>
            <div className="runtime-list">
              {runtimes.map((runtime) => (
                <div className="runtime-row" key={runtime.name}>
                  <span className={runtime.installed ? 'runtime-ready' : 'runtime-missing'} aria-hidden="true" />
                  <div>
                    <strong>{runtime.name}</strong>
                    <small>
                      {runtime.installed
                        ? runtime.version ?? runtime.binary ?? t('installed')
                        : runtime.unavailable_reason ?? t('unavailable')}
                    </small>
                  </div>
                </div>
              ))}
              {runtimes.length === 0 && <p className="quiet-copy">{t('noRuntimes')}</p>}
            </div>
          </section>
        </aside>

        <section className="conversation" aria-label={t('conversation')}>
          <header className="conversation-header">
            <div>
              <span className="eyebrow">{t('localChannel')}</span>
              <h1>{activeChannel ? `# ${activeChannel.name}` : t('chooseChannel')}</h1>
              {activeChannel?.description && <p>{activeChannel.description}</p>}
              {activeChannel?.goal && <small>{t('channelGoal')}: {activeChannel.goal}</small>}
            </div>
            {activeChannel && <span>{agentCountLabel}</span>}
          </header>

          <div className="message-stream" aria-live="polite">
            {showOnboarding && (
              <section className="onboarding-card" aria-labelledby="onboarding-title">
                <div>
                  <span className="eyebrow">{t('onboardingEyebrow')}</span>
                  <h2 id="onboarding-title">{t('onboardingTitle')}</h2>
                  <p>{t('onboardingDescription')}</p>
                </div>
                <ol>
                  {onboardingSteps.map((step) => (
                    <li className={step.done ? 'done' : ''} key={step.label}>
                      {step.done
                        ? <CheckCircle2 size={16} aria-hidden="true" />
                        : <Circle size={16} aria-hidden="true" />}
                      <span>{step.label}</span>
                    </li>
                  ))}
                </ol>
              </section>
            )}
            {!activeChannel && (
              <div className="empty-state">
                <span>01</span>
                <h2>{t('createChannel')}</h2>
                <p>{t('createChannelDescription')}</p>
              </div>
            )}
            {activeChannel && messagesLoading && <p className="stream-note">{t('loadingMessages')}</p>}
            {activeChannel && !messagesLoading && messages.length === 0 && (
              <div className="empty-state">
                <span>02</span>
                <h2>{t('startTask')}</h2>
                <p>{t('startTaskDescription')}</p>
              </div>
            )}
            {messages.map((message) => (
              <article
                id={`local-message-${message.id}`}
                className={`message ${message.role} ${pendingMessageId === message.id ? 'history-target' : ''}`}
                key={message.id}
              >
                <div className="message-meta">
                  <strong>
                    {message.role === 'user'
                      ? t('you')
                      : message.agent_id ? agentNames.get(message.agent_id) ?? t('agent') : t('agent')}
                  </strong>
                  <span>#{message.seq}</span>
                  <time dateTime={message.created_at}>{formatTime(message.created_at)}</time>
                </div>
                <p>{message.content}</p>
              </article>
            ))}
            {Object.values(liveTurns).map((turn) => (
              <article className={`live-turn ${turn.phase}`} key={turn.agentId}>
                <header>
                  <div>
                    <CircleDot size={14} aria-hidden="true" />
                    <strong>{agentNames.get(turn.agentId) ?? t('agent')}</strong>
                  </div>
                  <div className="live-turn-actions">
                    <span>{turn.phase === 'limited' ? t('liveLimited') : turn.phase === 'error' ? t('liveError') : t('liveRunning')}</span>
                    {turn.canCancel && (
                      <button
                        type="button"
                        disabled={pending === `cancel-${turn.agentId}`}
                        onClick={() => void cancelLiveTurn(turn.agentId)}
                      >
                        {pending === `cancel-${turn.agentId}` ? t('liveCancelling') : t('liveCancel')}
                      </button>
                    )}
                  </div>
                </header>
                {turn.thinking && !turn.text && <p className="live-thinking">{turn.thinking}</p>}
                {turn.text && <p className="live-output">{turn.text}</p>}
                {turn.activity.length > 0 && (
                  <ul>
                    {turn.activity.map((activity, index) => (
                      <li key={`${activity}-${index}`}>
                        <Wrench size={12} aria-hidden="true" />
                        {activity}
                      </li>
                    ))}
                  </ul>
                )}
              </article>
            ))}
            <div ref={messageEndRef} />
          </div>

          <form className="composer" onSubmit={sendMessage}>
            <label htmlFor="task-message">
              {activeChannel ? t('taskFor', { channel: activeChannel.name }) : t('selectChannelFirst')}
            </label>
            <textarea
              id="task-message"
              value={draft}
              onChange={(event) => setDraft(event.target.value)}
              placeholder={t('taskPlaceholder')}
              disabled={!activeChannel}
              rows={3}
            />
            <div>
              <span>
                {channelAgents.some((agent) => agent.status === 'running')
                  ? t('runningAgentsHint')
                  : t('noRunningAgentHint')}
              </span>
              <button
                type="submit"
                disabled={!activeChannel || !draft.trim() || pending === 'message'}
              >
                {pending === 'message' ? t('running') : t('runTask')}
              </button>
            </div>
          </form>
        </section>

        <aside className="agent-panel" aria-label={t('channelAgents')}>
          <div className="section-heading">
            <h2>{t('agents')}</h2>
            <span>{channelAgents.length}</span>
          </div>

          <div className="agent-list">
            {channelAgents.map((agent) => (
              <article className="agent-row" key={agent.id}>
                <div className="agent-title">
                  <span className={`agent-state-dot ${agent.lifecycle_status}`} aria-hidden="true" />
                  <strong>{agent.name}</strong>
                </div>
                <dl>
                  <div><dt>{t('runtime')}</dt><dd>{agent.runtime}</dd></div>
                  <div><dt>{t('model')}</dt><dd>{agent.model ?? t('defaultModel')}</dd></div>
                  <div><dt>{t('lifecycle')}</dt><dd>{agent.lifecycle_status}</dd></div>
                </dl>
                <button
                  type="button"
                  onClick={() => {
                    setActiveAgentId(agent.id)
                    setWorkspaceView('agent')
                  }}
                >
                  {t('conversation')}
                </button>
                <button
                  type="button"
                  disabled={pending === agent.id}
                  onClick={() => void setAgentStatus(
                    agent.id,
                    agent.lifecycle_status === 'active' ? 'stopped' : 'running',
                  )}
                >
                  {agent.lifecycle_status === 'active' ? t('pauseAgent') : t('resumeAgent')}
                </button>
              </article>
            ))}
            {activeChannel && channelAgents.length === 0 && (
              <p className="quiet-copy">{t('noAgents')}</p>
            )}
          </div>

          {activeChannel && availableChannelAgents.length > 0 && (
            <div className="channel-member-add">
              <LocalSelect
                id="existing-agent"
                ariaLabel={t('agentSelect')}
                value={inviteAgentId}
                options={availableChannelAgents.map((agent) => ({
                  value: agent.id,
                  label: agent.name,
                  meta: agent.runtime,
                }))}
                onChange={setInviteAgentId}
                placeholder={t('selectOption')}
              />
              <button
                type="button"
                disabled={!inviteAgentId || pending === 'join-agent'}
                onClick={() => void joinAgentToActiveChannel()}
              >
                {t('add')}
              </button>
            </div>
          )}

          <form className="agent-create" onSubmit={createAgent}>
            <h3>{t('addAgent')}</h3>
            <label htmlFor="agent-name">{t('name')}</label>
            <input
              id="agent-name"
              value={agentName}
              onChange={(event) => setAgentName(event.target.value)}
              placeholder={t('agentPlaceholder')}
              autoComplete="off"
              disabled={installedRuntimes.length === 0}
            />
            <label htmlFor="agent-description">{t('description')}</label>
            <input
              id="agent-description"
              value={agentDescription}
              onChange={(event) => setAgentDescription(event.target.value)}
              placeholder={t('agentDescriptionPlaceholder')}
              disabled={installedRuntimes.length === 0}
            />
            <label htmlFor="agent-instructions">{t('instructions')}</label>
            <textarea
              id="agent-instructions"
              value={agentInstructions}
              onChange={(event) => setAgentInstructions(event.target.value)}
              placeholder={t('agentInstructionsPlaceholder')}
              rows={3}
              disabled={installedRuntimes.length === 0}
            />
            <label htmlFor="runtime-name">{t('runtimeLabel')}</label>
            <LocalSelect
              id="runtime-name"
              ariaLabel={t('runtimeLabel')}
              value={runtimeName}
              options={runtimeOptions}
              onChange={selectRuntime}
              disabled={installedRuntimes.length === 0}
              placeholder={installedRuntimes.length === 0 ? t('noRuntimeInstalled') : t('selectOption')}
            />
            <label htmlFor="model-name">{t('modelLabel')}</label>
            {modelOptions.length > 0 ? (
              <LocalSelect
                id="model-name"
                ariaLabel={t('modelLabel')}
                value={model}
                options={modelOptions}
                onChange={setModel}
                disabled={installedRuntimes.length === 0}
                placeholder={t('selectOption')}
              />
            ) : (
              <input
                id="model-name"
                value={model}
                onChange={(event) => setModel(event.target.value)}
                placeholder={t('runtimeDefault')}
                disabled={installedRuntimes.length === 0}
              />
            )}
            <button
              type="submit"
              disabled={
                !agentName.trim()
                || !runtimeName
                || pending === 'agent'
              }
            >
              {pending === 'agent' ? t('adding') : t('addRunningAgent')}
            </button>
          </form>

          {activeChannel && (
            <section className="channel-workspace-attachments">
              <div className="section-heading">
                <h2>{t('workspaceAttachments')}</h2>
                <span>{channelWorkspaces.length}</span>
              </div>
              {channelWorkspaces.length > 0 && (
                <ul>
                  {channelWorkspaces.map((workspace) => (
                    <li key={workspace.id}>
                      <strong>{workspace.display_name}</strong>
                      <span>{workspace.provider_key}{workspace.portable_locator ? ` · ${workspace.portable_locator}` : ''}</span>
                    </li>
                  ))}
                </ul>
              )}
              <form onSubmit={attachChannelWorkspace}>
                <LocalSelect
                  id="channel-workspace-kind"
                  ariaLabel={t('workspaceKind')}
                  value={channelWorkspaceKind}
                  options={[
                    { value: 'directory', label: t('workspaceDirectory') },
                    { value: 'git', label: t('workspaceGit') },
                    { value: 'external', label: t('workspaceExternal') },
                    { value: 'managed', label: t('workspaceManaged') },
                  ]}
                  onChange={(value) => setChannelWorkspaceKind(value as BuiltInWorkspaceProviderKey)}
                  placeholder={t('selectOption')}
                />
                <input
                  aria-label={t('channelWorkspaceLocator')}
                  value={channelWorkspaceLocator}
                  onChange={(event) => setChannelWorkspaceLocator(event.target.value)}
                  placeholder={t('workspaceLocatorPlaceholder')}
                />
                <button
                  type="submit"
                  disabled={!channelWorkspaceLocator.trim() || pending === 'channel-workspace'}
                >
                  {pending === 'channel-workspace' ? t('adding') : t('attachWorkspace')}
                </button>
              </form>
            </section>
          )}
        </aside>
      </div>
      ) : workspaceView === 'agent' ? (
        <div className="agent-subject-layout">
          <aside className="agent-subject-rail" aria-label={t('agents')}>
            <div className="section-heading">
              <h2>{t('agents')}</h2>
              <span>{agents.length}</span>
            </div>
            <nav className="agent-subject-list">
              {agents.map((agent) => (
                <button
                  type="button"
                  key={agent.id}
                  className={agent.id === activeAgentId ? 'active' : ''}
                  onClick={() => setActiveAgentId(agent.id)}
                >
                  <span className={`agent-state-dot ${agent.lifecycle_status}`} aria-hidden="true" />
                  <span>
                    <strong>{agent.name}</strong>
                    <small>{agent.runtime} · {agent.lifecycle_status}</small>
                  </span>
                </button>
              ))}
              {agents.length === 0 && <p className="quiet-copy">{t('agentSelect')}</p>}
            </nav>
            <form className="agent-create agent-subject-create" onSubmit={createAgent}>
              <h3>{t('createStandaloneAgent')}</h3>
              <label htmlFor="standalone-agent-name">{t('name')}</label>
              <input
                id="standalone-agent-name"
                value={agentName}
                onChange={(event) => setAgentName(event.target.value)}
                placeholder={t('agentPlaceholder')}
                autoComplete="off"
              />
              <label htmlFor="standalone-agent-description">{t('description')}</label>
              <input
                id="standalone-agent-description"
                value={agentDescription}
                onChange={(event) => setAgentDescription(event.target.value)}
                placeholder={t('agentDescriptionPlaceholder')}
              />
              <label htmlFor="standalone-agent-instructions">{t('instructions')}</label>
              <textarea
                id="standalone-agent-instructions"
                value={agentInstructions}
                onChange={(event) => setAgentInstructions(event.target.value)}
                placeholder={t('agentInstructionsPlaceholder')}
                rows={3}
              />
              <label htmlFor="standalone-runtime">{t('runtimeLabel')}</label>
              <LocalSelect
                id="standalone-runtime"
                ariaLabel={t('runtimeLabel')}
                value={runtimeName}
                options={runtimeOptions}
                onChange={selectRuntime}
                placeholder={t('selectOption')}
              />
              <button type="submit" disabled={!agentName.trim() || !runtimeName || pending === 'agent'}>
                {pending === 'agent' ? t('adding') : t('addAgent')}
              </button>
            </form>
          </aside>

          <section className="agent-subject-detail" aria-label={t('agentOverview')}>
            {!activeAgent && (
              <div className="empty-state">
                <span>@</span>
                <h1>{t('agentSelect')}</h1>
              </div>
            )}
            {activeAgent && (
              <>
                <header className="agent-subject-header">
                  <div>
                    <span className="workspace-eyebrow">{t('agentOverview')}</span>
                    <h1>@{activeAgent.name}</h1>
                    <p>{activeAgent.description || activeAgent.instructions || `${activeAgent.runtime} · ${activeAgent.model ?? t('defaultModel')}`}</p>
                  </div>
                  <dl>
                    <div><dt>{t('lifecycle')}</dt><dd>{activeAgent.lifecycle_status}</dd></div>
                    <div><dt>{t('runtime')}</dt><dd>{activeAgent.runtime}</dd></div>
                    <div><dt>{t('model')}</dt><dd>{activeAgent.model ?? t('defaultModel')}</dd></div>
                  </dl>
                  <button
                    type="button"
                    className="agent-lifecycle-control"
                    disabled={pending === activeAgent.id}
                    onClick={() => void setAgentStatus(
                      activeAgent.id,
                      activeAgent.lifecycle_status === 'active' ? 'stopped' : 'running',
                    )}
                  >
                    {activeAgent.lifecycle_status === 'active' ? t('pauseAgent') : t('resumeAgent')}
                  </button>
                </header>

                <div className="agent-context-grid">
                  <article>
                    <h2>{t('currentWork')}</h2>
                    {agentWorkingState ? (
                      <>
                        <strong>{agentWorkingState.summary}</strong>
                        {agentWorkingState.channel_name && <p>#{agentWorkingState.channel_name}</p>}
                        {agentWorkingState.next_step_hint && (
                          <p><b>{t('nextStep')}:</b> {agentWorkingState.next_step_hint}</p>
                        )}
                      </>
                    ) : <p>{t('agentIdle')}</p>}
                  </article>
                  <article>
                    <h2>{t('memberChannels')}</h2>
                    {agentChannels.length > 0
                      ? <ul>{agentChannels.map((channel) => <li key={channel.id}>#{channel.name}</li>)}</ul>
                      : <p>{t('noChannels')}</p>}
                  </article>
                  <article>
                    <h2>{t('workspaceAttachments')}</h2>
                    {agentWorkspaces.length > 0
                      ? <ul>{agentWorkspaces.map((workspace) => (
                        <li key={workspace.id}>{workspace.display_name} · {workspace.provider_key}{workspace.portable_locator ? ` · ${workspace.portable_locator}` : ''}</li>
                      ))}</ul>
                      : <p>{t('noWorkspaces')}</p>}
                    <form className="workspace-attach-form" onSubmit={attachAgentWorkspace}>
                      <LocalSelect
                        id="agent-workspace-kind"
                        ariaLabel={t('workspaceKind')}
                        value={workspaceKind}
                        options={[
                          { value: 'directory', label: t('workspaceDirectory') },
                          { value: 'git', label: t('workspaceGit') },
                          { value: 'external', label: t('workspaceExternal') },
                          { value: 'managed', label: t('workspaceManaged') },
                        ]}
                        onChange={(value) => setWorkspaceKind(value as BuiltInWorkspaceProviderKey)}
                        placeholder={t('selectOption')}
                      />
                      <input
                        aria-label={t('workspaceLocator')}
                        value={workspaceLocator}
                        onChange={(event) => setWorkspaceLocator(event.target.value)}
                        placeholder={t('workspaceLocatorPlaceholder')}
                      />
                      <button
                        type="submit"
                        disabled={!workspaceLocator.trim() || pending === 'agent-workspace'}
                      >
                        {pending === 'agent-workspace' ? t('adding') : t('attachWorkspace')}
                      </button>
                    </form>
                  </article>
                  <article>
                    <h2>{t('operationHistory')}</h2>
                    {agentOperations.length > 0
                      ? <ul>{agentOperations.slice(0, 5).map((operation) => (
                        <li key={operation.id}>{operation.action} · {formatTime(operation.created_at)}</li>
                      ))}</ul>
                      : <p>{t('noOperations')}</p>}
                  </article>
                </div>

                <section className="agent-direct-conversation">
                  <header><h2>{t('agentDirect')}</h2></header>
                  <div className="agent-direct-messages" aria-live="polite">
                    {agentMessages.length === 0 && <p>{t('agentNoMessages')}</p>}
                    {agentMessages.map((message) => (
                      <article className={`message ${message.role}`} key={message.id}>
                        <div className="message-meta">
                          <strong>{message.role === 'user' ? t('you') : activeAgent.name}</strong>
                          <span>#{message.seq}</span>
                          <time dateTime={message.created_at}>{formatTime(message.created_at)}</time>
                        </div>
                        <p>{message.content}</p>
                      </article>
                    ))}
                  </div>
                  <form className="agent-direct-composer" onSubmit={sendAgentMessage}>
                    <textarea
                      value={agentDraft}
                      onChange={(event) => setAgentDraft(event.target.value)}
                      placeholder={t('agentDirectPlaceholder')}
                      rows={3}
                    />
                    <button
                      type="submit"
                      disabled={!agentDraft.trim() || pending === 'agent-message'}
                    >
                      {pending === 'agent-message' ? t('running') : t('runTask')}
                    </button>
                  </form>
                </section>
              </>
            )}
          </section>
        </div>
      ) : workspaceView === 'agent-memory' ? (
        <LocalKnowledgeWorkspace agents={agents} t={t} />
      ) : workspaceView === 'tasks' ? (
        <LocalTasksWorkspace
          channels={channels}
          agents={agents}
          activeChannelId={activeChannelId}
          onChannelChange={setActiveChannelId}
          t={t}
        />
      ) : workspaceView === 'knowledge' ? (
        <LocalKnowledgeWorkspace agents={agents} t={t} />
      ) : workspaceView === 'history' ? (
        <LocalHistoryWorkspace
          agents={agents}
          onOpenMessage={(channelId, messageId) => {
            setActiveChannelId(channelId)
            setPendingMessageId(messageId)
            setWorkspaceView('chat')
          }}
          t={t}
        />
      ) : workspaceView === 'mcp' ? (
        <LocalMcpWorkspace t={t} />
      ) : (
        workspaceView === 'settings' ? (
          <section className="local-settings-workspace" aria-label={t('settings')}>
            <header className="workspace-section-header">
              <span className="workspace-eyebrow">{t('settings')}</span>
              <h1>{t('runtimeSettings')}</h1>
              <p>{t('runtimeSettingsDescription')}</p>
            </header>
            <div className="runtime-settings-grid">
              {runtimes.map((runtime) => (
                <article key={runtime.name} className="runtime-settings-card">
                  <strong>{runtime.name}</strong>
                  <span>{runtime.installed ? t('installed') : t('unavailable')}</span>
                  <p>{runtime.version ?? runtime.unavailable_reason ?? runtime.binary ?? '—'}</p>
                </article>
              ))}
            </div>
          </section>
        ) : (
        <LocalSkillsWorkspace agents={agents} t={t} />
        )
      )}
    </main>
  )
}

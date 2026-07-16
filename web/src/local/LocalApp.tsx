import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
} from 'react'
import { Languages, MessageSquare, Moon, PackageOpen, Sun } from 'lucide-react'
import {
  localApi,
  type Agent,
  type AgentStatus,
  type Channel,
  type Message,
  type RuntimeInfo,
} from './api'
import { LocalSelect } from './LocalSelect'
import { LocalSkillsWorkspace } from './LocalSkillsWorkspace'
import {
  LANGUAGE_OPTIONS,
  resolveInitialLanguage,
  translate,
  type LocalCopyKey,
  type LocalLanguage,
} from './localization'

type LocalTheme = 'light' | 'dark'
type WorkspaceView = 'chat' | 'skills'

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
  const [activeChannelId, setActiveChannelId] = useState<string | null>(null)
  const [channelName, setChannelName] = useState('')
  const [agentName, setAgentName] = useState('')
  const [runtimeName, setRuntimeName] = useState('')
  const [model, setModel] = useState('')
  const [draft, setDraft] = useState('')
  const [loading, setLoading] = useState(true)
  const [messagesLoading, setMessagesLoading] = useState(false)
  const [pending, setPending] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const messageEndRef = useRef<HTMLDivElement>(null)
  const t = useCallback(
    (key: LocalCopyKey, values?: Record<string, string | number>) =>
      translate(language, key, values),
    [language],
  )

  const activeChannel = useMemo(
    () => channels.find((channel) => channel.id === activeChannelId) ?? null,
    [activeChannelId, channels],
  )
  const channelAgents = useMemo(
    () => agents.filter((agent) => agent.channel_id === activeChannelId),
    [activeChannelId, agents],
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
      setMessages([])
      return
    }
    let cancelled = false
    setMessagesLoading(true)
    localApi.listMessages(activeChannelId)
      .then((nextMessages) => {
        if (!cancelled) setMessages(nextMessages)
      })
      .catch((nextError: unknown) => {
        if (!cancelled) setError(errorMessage(nextError))
      })
      .finally(() => {
        if (!cancelled) setMessagesLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [activeChannelId])

  useEffect(() => {
    messageEndRef.current?.scrollIntoView({ behavior: 'smooth', block: 'end' })
  }, [messages])

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
      const channel = await localApi.createChannel(name)
      setChannels((current) => [...current, channel])
      setActiveChannelId(channel.id)
      setChannelName('')
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
    if (!activeChannelId || !agentName.trim() || !runtimeName) return
    setPending('agent')
    setError(null)
    try {
      const agent = await localApi.createAgent({
        channel_id: activeChannelId,
        name: agentName.trim(),
        runtime: runtimeName,
        model: model || null,
      })
      setAgents((current) => [...current, agent])
      setAgentName('')
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

  if (loading) {
    return <main className="local-loading">{t('loading')}</main>
  }

  const agentCountLabel = t(
    channelAgents.length === 1 ? 'agentCount' : 'agentCountPlural',
    { count: channelAgents.length },
  )

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
            className={workspaceView === 'chat' ? 'active' : ''}
            aria-current={workspaceView === 'chat' ? 'page' : undefined}
            onClick={() => setWorkspaceView('chat')}
          >
            <MessageSquare size={14} aria-hidden="true" />
            {t('chatWorkspace')}
          </button>
          <button
            type="button"
            className={workspaceView === 'skills' ? 'active' : ''}
            aria-current={workspaceView === 'skills' ? 'page' : undefined}
            onClick={() => setWorkspaceView('skills')}
          >
            <PackageOpen size={14} aria-hidden="true" />
            {t('skillsWorkspace')}
          </button>
        </nav>

        <div className="topbar-actions">
          <div className="service-status">
            <span className="status-dot" aria-hidden="true" />
            {t('serviceOnline')}
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
              <div>
                <input
                  id="channel-name"
                  value={channelName}
                  onChange={(event) => setChannelName(event.target.value)}
                  placeholder={t('channelPlaceholder')}
                  autoComplete="off"
                />
                <button type="submit" disabled={!channelName.trim() || pending === 'channel'}>
                  {t('add')}
                </button>
              </div>
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
            </div>
            {activeChannel && <span>{agentCountLabel}</span>}
          </header>

          <div className="message-stream" aria-live="polite">
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
              <article className={`message ${message.role}`} key={message.id}>
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
                  <span className={agent.status === 'running' ? 'agent-running' : 'agent-stopped'} aria-hidden="true" />
                  <strong>{agent.name}</strong>
                </div>
                <dl>
                  <div><dt>{t('runtime')}</dt><dd>{agent.runtime}</dd></div>
                  <div><dt>{t('model')}</dt><dd>{agent.model ?? t('defaultModel')}</dd></div>
                  <div><dt>{t('state')}</dt><dd>{agent.status}</dd></div>
                </dl>
                <button
                  type="button"
                  disabled={pending === agent.id}
                  onClick={() => void setAgentStatus(
                    agent.id,
                    agent.status === 'running' ? 'stopped' : 'running',
                  )}
                >
                  {agent.status === 'running' ? t('stopDelivery') : t('startAgent')}
                </button>
              </article>
            ))}
            {activeChannel && channelAgents.length === 0 && (
              <p className="quiet-copy">{t('noAgents')}</p>
            )}
          </div>

          <form className="agent-create" onSubmit={createAgent}>
            <h3>{t('addAgent')}</h3>
            <label htmlFor="agent-name">{t('name')}</label>
            <input
              id="agent-name"
              value={agentName}
              onChange={(event) => setAgentName(event.target.value)}
              placeholder={t('agentPlaceholder')}
              autoComplete="off"
              disabled={!activeChannel}
            />
            <label htmlFor="runtime-name">{t('runtimeLabel')}</label>
            <LocalSelect
              id="runtime-name"
              ariaLabel={t('runtimeLabel')}
              value={runtimeName}
              options={runtimeOptions}
              onChange={selectRuntime}
              disabled={!activeChannel || installedRuntimes.length === 0}
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
                disabled={!activeChannel}
                placeholder={t('selectOption')}
              />
            ) : (
              <input
                id="model-name"
                value={model}
                onChange={(event) => setModel(event.target.value)}
                placeholder={t('runtimeDefault')}
                disabled={!activeChannel}
              />
            )}
            <button
              type="submit"
              disabled={
                !activeChannel
                || !agentName.trim()
                || !runtimeName
                || pending === 'agent'
              }
            >
              {pending === 'agent' ? t('adding') : t('addRunningAgent')}
            </button>
          </form>
        </aside>
      </div>
      ) : (
        <LocalSkillsWorkspace agents={agents} t={t} />
      )}
    </main>
  )
}

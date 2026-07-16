import { useEffect, useMemo, useRef, useState, type FormEvent } from 'react'
import {
  localApi,
  type Agent,
  type AgentStatus,
  type Channel,
  type Message,
  type RuntimeInfo,
} from './api'

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : 'Unexpected local service error'
}

function formatTime(value: string): string {
  return new Intl.DateTimeFormat(undefined, {
    hour: '2-digit',
    minute: '2-digit',
  }).format(new Date(value))
}

export function LocalApp() {
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
    return <main className="local-loading">Starting local workspace…</main>
  }

  return (
    <main className="local-shell">
      <header className="local-topbar">
        <div>
          <span className="brand-mark" aria-hidden="true">c</span>
          <strong>cocli local</strong>
        </div>
        <div className="service-status">
          <span className="status-dot" aria-hidden="true" />
          SQLite + HTTP online
        </div>
      </header>

      {error && (
        <div className="error-banner" role="alert">
          <span>{error}</span>
          <button type="button" onClick={() => setError(null)}>Dismiss</button>
        </div>
      )}

      <div className="local-grid">
        <aside className="channel-rail" aria-label="Channels and runtimes">
          <section>
            <div className="section-heading">
              <h2>Channels</h2>
              <span>{channels.length}</span>
            </div>
            <nav className="channel-list" aria-label="Channels">
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
              {channels.length === 0 && (
                <p className="quiet-copy">Create a channel to open your first local workspace.</p>
              )}
            </nav>
            <form className="inline-create" onSubmit={createChannel}>
              <label htmlFor="channel-name">New channel</label>
              <div>
                <input
                  id="channel-name"
                  value={channelName}
                  onChange={(event) => setChannelName(event.target.value)}
                  placeholder="product-loop"
                  autoComplete="off"
                />
                <button type="submit" disabled={!channelName.trim() || pending === 'channel'}>
                  Add
                </button>
              </div>
            </form>
          </section>

          <section className="runtime-section">
            <div className="section-heading">
              <h2>Local runtimes</h2>
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
                        ? runtime.version ?? runtime.binary ?? 'installed'
                        : runtime.unavailable_reason ?? 'unavailable'}
                    </small>
                  </div>
                </div>
              ))}
              {runtimes.length === 0 && (
                <p className="quiet-copy">No runtime catalog is configured.</p>
              )}
            </div>
          </section>
        </aside>

        <section className="conversation" aria-label="Channel conversation">
          <header className="conversation-header">
            <div>
              <span className="eyebrow">LOCAL CHANNEL</span>
              <h1>{activeChannel ? `# ${activeChannel.name}` : 'Choose a channel'}</h1>
            </div>
            {activeChannel && <span>{channelAgents.length} agent{channelAgents.length === 1 ? '' : 's'}</span>}
          </header>

          <div className="message-stream" aria-live="polite">
            {!activeChannel && (
              <div className="empty-state">
                <span>01</span>
                <h2>Create a channel</h2>
                <p>Your channels, agents, and message history stay in the local SQLite database.</p>
              </div>
            )}
            {activeChannel && messagesLoading && <p className="stream-note">Loading message history…</p>}
            {activeChannel && !messagesLoading && messages.length === 0 && (
              <div className="empty-state">
                <span>02</span>
                <h2>Start a real task</h2>
                <p>Add an installed runtime agent, then send a concrete prompt from the composer below.</p>
              </div>
            )}
            {messages.map((message) => (
              <article className={`message ${message.role}`} key={message.id}>
                <div className="message-meta">
                  <strong>
                    {message.role === 'user'
                      ? 'you'
                      : message.agent_id ? agentNames.get(message.agent_id) ?? 'agent' : 'agent'}
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
              {activeChannel ? `Task for #${activeChannel.name}` : 'Select a channel first'}
            </label>
            <textarea
              id="task-message"
              value={draft}
              onChange={(event) => setDraft(event.target.value)}
              placeholder="Describe the task, constraints, and expected result…"
              disabled={!activeChannel}
              rows={3}
            />
            <div>
              <span>
                {channelAgents.some((agent) => agent.status === 'running')
                  ? 'Running agents will receive this task.'
                  : 'Add or start an agent before sending.'}
              </span>
              <button
                type="submit"
                disabled={!activeChannel || !draft.trim() || pending === 'message'}
              >
                {pending === 'message' ? 'Running…' : 'Run task'}
              </button>
            </div>
          </form>
        </section>

        <aside className="agent-panel" aria-label="Channel agents">
          <div className="section-heading">
            <h2>Agents</h2>
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
                  <div><dt>runtime</dt><dd>{agent.runtime}</dd></div>
                  <div><dt>model</dt><dd>{agent.model ?? 'default'}</dd></div>
                  <div><dt>state</dt><dd>{agent.status}</dd></div>
                </dl>
                <button
                  type="button"
                  disabled={pending === agent.id}
                  onClick={() => void setAgentStatus(
                    agent.id,
                    agent.status === 'running' ? 'stopped' : 'running',
                  )}
                >
                  {agent.status === 'running' ? 'Stop delivery' : 'Start agent'}
                </button>
              </article>
            ))}
            {activeChannel && channelAgents.length === 0 && (
              <p className="quiet-copy">No agent is attached to this channel yet.</p>
            )}
          </div>

          <form className="agent-create" onSubmit={createAgent}>
            <h3>Add agent</h3>
            <label htmlFor="agent-name">Name</label>
            <input
              id="agent-name"
              value={agentName}
              onChange={(event) => setAgentName(event.target.value)}
              placeholder="builder"
              autoComplete="off"
              disabled={!activeChannel}
            />
            <label htmlFor="runtime-name">Runtime</label>
            <select
              id="runtime-name"
              value={runtimeName}
              onChange={(event) => selectRuntime(event.target.value)}
              disabled={!activeChannel || installedRuntimes.length === 0}
            >
              {installedRuntimes.length === 0 && <option value="">No runtime installed</option>}
              {installedRuntimes.map((runtime) => (
                <option key={runtime.name} value={runtime.name}>{runtime.name}</option>
              ))}
            </select>
            <label htmlFor="model-name">Model</label>
            {selectedRuntime?.models.length ? (
              <select
                id="model-name"
                value={model}
                onChange={(event) => setModel(event.target.value)}
              >
                {selectedRuntime.models.map((runtimeModel) => (
                  <option key={runtimeModel} value={runtimeModel}>{runtimeModel}</option>
                ))}
              </select>
            ) : (
              <input
                id="model-name"
                value={model}
                onChange={(event) => setModel(event.target.value)}
                placeholder="runtime default"
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
              {pending === 'agent' ? 'Adding…' : 'Add running agent'}
            </button>
          </form>
        </aside>
      </div>
    </main>
  )
}

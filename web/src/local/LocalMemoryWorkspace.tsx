import {
  ArrowLeftRight,
  Brain,
  Plus,
  RefreshCw,
  Save,
} from 'lucide-react'
import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type FormEvent,
} from 'react'
import {
  localApi,
  type Agent,
  type Channel,
  type MemoryDocumentEntry,
  type MemoryScope,
  type MemoryTopic,
  type MemoryType,
} from './api'
import { LocalSelect } from './LocalSelect'
import type { LocalCopyKey } from './localization'

interface LocalMemoryWorkspaceProps {
  agents: Agent[]
  t: (key: LocalCopyKey, values?: Record<string, string | number>) => string
}

interface MemoryEntry extends MemoryDocumentEntry {
  type: MemoryType
  topic: string
}

const MEMORY_TYPES: MemoryType[] = ['project', 'user', 'feedback', 'reference']

function parseMemoryEntry(entry: MemoryDocumentEntry): MemoryEntry | null {
  const filename = entry.path.split('/').at(-1)
  if (!filename || filename === 'MEMORY.md' || !filename.endsWith('.md')) return null
  const stem = filename.slice(0, -3)
  const separator = stem.indexOf('_')
  if (separator < 1) return null
  const type = stem.slice(0, separator)
  if (!MEMORY_TYPES.includes(type as MemoryType)) return null
  return {
    ...entry,
    type: type as MemoryType,
    topic: stem.slice(separator + 1),
  }
}

function editableMemoryBody(body: string): string {
  if (!body.startsWith('---\n')) return body
  const close = body.indexOf('\n---\n', 4)
  return close < 0 ? body : body.slice(close + 5).replace(/^\n+/, '')
}

export function LocalMemoryWorkspace({
  agents,
  t,
}: LocalMemoryWorkspaceProps) {
  const [agentId, setAgentId] = useState(agents[0]?.id ?? '')
  const [memberChannels, setMemberChannels] = useState<Channel[]>([])
  const [channelId, setChannelId] = useState('')
  const [scope, setScope] = useState<MemoryScope>('agent')
  const [entries, setEntries] = useState<MemoryEntry[]>([])
  const [selectedKey, setSelectedKey] = useState('')
  const [topic, setTopic] = useState<MemoryTopic | null>(null)
  const [filter, setFilter] = useState('')
  const [typeFilter, setTypeFilter] = useState<'all' | MemoryType>('all')
  const [draftDescription, setDraftDescription] = useState('')
  const [draftBody, setDraftBody] = useState('')
  const [createOpen, setCreateOpen] = useState(false)
  const [newType, setNewType] = useState<MemoryType>('project')
  const [newTopic, setNewTopic] = useState('')
  const [newDescription, setNewDescription] = useState('')
  const [newBody, setNewBody] = useState('')
  const [loading, setLoading] = useState(false)
  const [action, setAction] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  const selectedAgent = useMemo(
    () => agents.find((agent) => agent.id === agentId) ?? null,
    [agentId, agents],
  )
  const selectedChannel = useMemo(
    () => memberChannels.find((channel) => channel.id === channelId) ?? null,
    [channelId, memberChannels],
  )
  const agentOptions = useMemo(
    () => agents.map((agent) => ({
      value: agent.id,
      label: agent.name,
    })),
    [agents],
  )
  const typeOptions = useMemo(
    () => MEMORY_TYPES.map((type) => ({
      value: type,
      label: t(`memoryType${type[0].toUpperCase()}${type.slice(1)}` as LocalCopyKey),
    })),
    [t],
  )
  const channelOptions = useMemo(
    () => memberChannels.map((channel) => ({
      value: channel.id,
      label: `# ${channel.name}`,
    })),
    [memberChannels],
  )
  const visibleEntries = useMemo(() => {
    const normalized = filter.trim().toLocaleLowerCase()
    return entries.filter((entry) => (
      (typeFilter === 'all' || entry.type === typeFilter)
      && (!normalized
        || entry.topic.toLocaleLowerCase().includes(normalized)
        || entry.type.includes(normalized)
        || entry.body.toLocaleLowerCase().includes(normalized))
    ))
  }, [entries, filter, typeFilter])

  const loadEntries = useCallback(async () => {
    if (!selectedAgent || (scope === 'channel' && !selectedChannel)) {
      setEntries([])
      setSelectedKey('')
      setTopic(null)
      return
    }
    setLoading(true)
    setError(null)
    try {
      const response = await localApi.listMemory(
        selectedAgent.id,
        scope,
        scope === 'channel' ? selectedChannel?.id : undefined,
      )
      const nextEntries = response.entries
        .map(parseMemoryEntry)
        .filter((entry): entry is MemoryEntry => entry !== null)
      setEntries(nextEntries)
      setSelectedKey((current) => (
        nextEntries.some((entry) => `${entry.type}:${entry.topic}` === current)
          ? current
          : ''
      ))
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : t('memoryLoadError'))
    } finally {
      setLoading(false)
    }
  }, [scope, selectedAgent, selectedChannel, t])

  useEffect(() => {
    setAgentId((current) => (
      agents.some((agent) => agent.id === current) ? current : agents[0]?.id ?? ''
    ))
  }, [agents])

  useEffect(() => {
    let cancelled = false
    if (!agentId) {
      setMemberChannels([])
      setChannelId('')
      return
    }
    void localApi.listAgentChannels(agentId)
      .then((nextChannels) => {
        if (cancelled) return
        const visibleChannels = nextChannels.filter((channel) => !channel.is_system)
        setMemberChannels(visibleChannels)
        setChannelId((current) => (
          visibleChannels.some((channel) => channel.id === current)
            ? current
            : visibleChannels[0]?.id ?? ''
        ))
      })
      .catch((nextError: unknown) => {
        if (!cancelled) {
          setError(nextError instanceof Error ? nextError.message : t('memoryLoadError'))
        }
      })
    return () => {
      cancelled = true
    }
  }, [agentId, t])

  useEffect(() => {
    setSelectedKey('')
    setTopic(null)
    void loadEntries()
  }, [loadEntries])

  useEffect(() => {
    if (!selectedAgent || !selectedKey) {
      setTopic(null)
      return
    }
    const [type, ...topicParts] = selectedKey.split(':')
    const topicSlug = topicParts.join(':')
    let cancelled = false
    setLoading(true)
    localApi.getMemoryTopic(
      selectedAgent.id,
      scope,
      type as MemoryType,
      topicSlug,
      scope === 'channel' ? selectedChannel?.id : undefined,
    )
      .then((nextTopic) => {
        if (cancelled) return
        setTopic(nextTopic)
        setDraftDescription(nextTopic.description)
        setDraftBody(editableMemoryBody(nextTopic.body))
      })
      .catch((nextError: unknown) => {
        if (!cancelled) {
          setError(nextError instanceof Error ? nextError.message : t('memoryLoadError'))
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [scope, selectedAgent, selectedChannel, selectedKey, t])

  const runAction = useCallback(async (key: string, task: () => Promise<void>) => {
    setAction(key)
    setError(null)
    try {
      await task()
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : t('memoryActionError'))
    } finally {
      setAction(null)
    }
  }, [t])

  function createTopic(event: FormEvent) {
    event.preventDefault()
    if (
      !selectedAgent
      || (scope === 'channel' && !selectedChannel)
      || !newTopic.trim()
      || !newBody.trim()
    ) return
    const slug = newTopic.trim().toLocaleLowerCase().replace(/[^a-z0-9_]+/g, '_')
    void runAction('create', async () => {
      const created = await localApi.writeMemoryTopic(selectedAgent.id, {
        scope,
        channelId: scope === 'channel' ? selectedChannel?.id : undefined,
        type: newType,
        topic: slug,
        description: newDescription.trim(),
        body: newBody.trim(),
      })
      setNewTopic('')
      setNewDescription('')
      setNewBody('')
      setCreateOpen(false)
      await loadEntries()
      setSelectedKey(`${created.type}:${created.topic}`)
    })
  }

  function saveTopic(event: FormEvent) {
    event.preventDefault()
    if (!selectedAgent || !topic || (scope === 'channel' && !selectedChannel)) return
    void runAction('save', async () => {
      const updated = await localApi.writeMemoryTopic(selectedAgent.id, {
        scope,
        channelId: scope === 'channel' ? selectedChannel?.id : undefined,
        type: topic.type,
        topic: topic.topic,
        description: draftDescription.trim(),
        body: draftBody,
        ifVersion: topic.version,
      })
      setTopic(updated)
      setDraftBody(editableMemoryBody(updated.body))
      await loadEntries()
    })
  }

  function moveTopic() {
    if (!selectedAgent || !topic || (scope === 'agent' && !selectedChannel)) return
    const destination: MemoryScope = scope === 'agent' ? 'channel' : 'agent'
    void runAction('move', async () => {
      await localApi.moveMemoryTopic(selectedAgent.id, {
        fromScope: scope,
        fromChannelId: scope === 'channel' ? selectedChannel?.id : undefined,
        toScope: destination,
        toChannelId: destination === 'channel' ? selectedChannel?.id : undefined,
        type: topic.type,
        topic: topic.topic,
      })
      setSelectedKey('')
      setTopic(null)
      await loadEntries()
    })
  }

  if (agents.length === 0) {
    return (
      <div className="knowledge-empty-state">
        <Brain size={28} aria-hidden="true" />
        <h2>{t('memoryNoAgent')}</h2>
        <p>{t('memoryNoAgentDescription')}</p>
      </div>
    )
  }

  return (
    <div className="memory-workspace">
      <aside className="knowledge-sidebar">
        <div className="knowledge-controls">
          <label>
            <span>{t('memoryAgent')}</span>
            <LocalSelect
              id="memory-agent"
              ariaLabel={t('memoryAgent')}
              value={agentId}
              options={agentOptions}
              onChange={setAgentId}
              placeholder={t('memorySelectAgent')}
            />
          </label>
          <div>
            <span className="control-label">{t('memoryScope')}</span>
            <div className="segmented-control" aria-label={t('memoryScope')}>
              <button
                type="button"
                className={scope === 'agent' ? 'active' : ''}
                onClick={() => setScope('agent')}
              >
                {t('memoryPrivate')}
              </button>
              <button
                type="button"
                className={scope === 'channel' ? 'active' : ''}
                disabled={memberChannels.length === 0}
                onClick={() => setScope('channel')}
              >
                {t('memoryShared')}
              </button>
            </div>
          </div>
          {scope === 'channel' && (
            <label>
              <span>{t('memoryChannel')}</span>
              <LocalSelect
                id="memory-channel"
                ariaLabel={t('memoryChannel')}
                value={channelId}
                options={channelOptions}
                onChange={setChannelId}
                placeholder={t('tasksSelectChannel')}
              />
            </label>
          )}
          <p className="scope-note">
            {scope === 'agent'
              ? t('memoryPrivateDescription', { agent: selectedAgent?.name ?? '' })
              : t('memorySharedDescription', { channel: selectedChannel?.name ?? '' })}
          </p>
          <div className="knowledge-filter-row">
            <input
              aria-label={t('memoryFilter')}
              value={filter}
              onChange={(event) => setFilter(event.target.value)}
              placeholder={t('memoryFilterPlaceholder')}
            />
            <select
              aria-label={t('memoryType')}
              value={typeFilter}
              onChange={(event) => setTypeFilter(event.target.value as 'all' | MemoryType)}
            >
              <option value="all">{t('memoryAllTypes')}</option>
              {typeOptions.map((option) => (
                <option key={option.value} value={option.value}>{option.label}</option>
              ))}
            </select>
          </div>
          <div className="knowledge-sidebar-actions">
            <button type="button" onClick={() => setCreateOpen((current) => !current)}>
              <Plus size={14} aria-hidden="true" />
              {t('memoryNewTopic')}
            </button>
            <button type="button" aria-label={t('refresh')} onClick={() => void loadEntries()}>
              <RefreshCw size={14} aria-hidden="true" />
            </button>
          </div>
        </div>

        {createOpen && (
          <form className="memory-create-form" onSubmit={createTopic}>
            <label>
              <span>{t('memoryType')}</span>
              <LocalSelect
                id="memory-new-type"
                ariaLabel={t('memoryType')}
                value={newType}
                options={typeOptions}
                onChange={(value) => setNewType(value as MemoryType)}
                placeholder={t('memoryType')}
              />
            </label>
            <label>
              <span>{t('memoryTopic')}</span>
              <input
                value={newTopic}
                onChange={(event) => setNewTopic(event.target.value)}
                placeholder={t('memoryTopicPlaceholder')}
                pattern="[A-Za-z0-9 _-]+"
                maxLength={60}
                required
              />
            </label>
            <label>
              <span>{t('memoryDescription')}</span>
              <input
                value={newDescription}
                onChange={(event) => setNewDescription(event.target.value)}
                placeholder={t('memoryDescriptionPlaceholder')}
                maxLength={150}
              />
            </label>
            <label>
              <span>{t('memoryBody')}</span>
              <textarea
                value={newBody}
                onChange={(event) => setNewBody(event.target.value)}
                placeholder={t('memoryBodyPlaceholder')}
                rows={5}
                required
              />
            </label>
            <button type="submit" disabled={action === 'create' || !newTopic.trim() || !newBody.trim()}>
              {action === 'create' ? t('memoryCreating') : t('memoryCreate')}
            </button>
          </form>
        )}

        <div className="knowledge-list" aria-label={t('memoryTopics')}>
          {loading && entries.length === 0 ? (
            <p className="knowledge-list-note">{t('memoryLoading')}</p>
          ) : visibleEntries.length === 0 ? (
            <p className="knowledge-list-note">{t(entries.length === 0 ? 'memoryEmpty' : 'memoryNoMatches')}</p>
          ) : visibleEntries.map((entry) => {
            const key = `${entry.type}:${entry.topic}`
            return (
              <button
                type="button"
                key={entry.path}
                className={selectedKey === key ? 'active' : ''}
                onClick={() => setSelectedKey(key)}
              >
                <span className={`memory-type memory-type-${entry.type}`}>{entry.type}</span>
                <strong>{entry.topic}</strong>
                <small>v{entry.version}</small>
              </button>
            )
          })}
        </div>
      </aside>

      <section className="knowledge-editor">
        {error && <div className="workspace-error" role="alert">{error}</div>}
        {!topic ? (
          <div className="knowledge-empty-state compact">
            <Brain size={26} aria-hidden="true" />
            <h2>{t('memorySelectTopic')}</h2>
            <p>{t('memorySelectTopicDescription')}</p>
          </div>
        ) : (
          <form className="knowledge-edit-form" onSubmit={saveTopic}>
            <header className="knowledge-editor-header">
              <div>
                <span className={`memory-type memory-type-${topic.type}`}>{topic.type}</span>
                <h2>{topic.topic}</h2>
                <p>{topic.path} · v{topic.version} · {topic.updated}</p>
              </div>
              <div>
                <button type="button" onClick={moveTopic} disabled={action !== null}>
                  <ArrowLeftRight size={14} aria-hidden="true" />
                  {scope === 'agent' ? t('memoryMoveToChannel') : t('memoryMoveToAgent')}
                </button>
                <button type="submit" className="primary" disabled={action !== null}>
                  <Save size={14} aria-hidden="true" />
                  {action === 'save' ? t('memorySaving') : t('memorySave')}
                </button>
              </div>
            </header>
            <label>
              <span>{t('memoryDescription')}</span>
              <input
                value={draftDescription}
                onChange={(event) => setDraftDescription(event.target.value)}
                maxLength={150}
              />
            </label>
            <label className="editor-body">
              <span>{t('memoryBody')}</span>
              <textarea
                value={draftBody}
                onChange={(event) => setDraftBody(event.target.value)}
                spellCheck
              />
            </label>
          </form>
        )}
      </section>
    </div>
  )
}

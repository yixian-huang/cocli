import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type DragEvent,
  type FormEvent,
} from 'react'
import {
  Columns3,
  Link2,
  List,
  Plus,
  RefreshCw,
  UserMinus,
  UserPlus,
  X,
} from 'lucide-react'
import {
  localApi,
  type Agent,
  type Channel,
  type Task,
  type TaskStatus,
} from './api'
import { LocalSelect } from './LocalSelect'
import type { LocalCopyKey } from './localization'
import './LocalTasksWorkspace.css'

interface LocalTasksWorkspaceProps {
  channels: Channel[]
  agents: Agent[]
  activeChannelId: string | null
  onChannelChange: (channelId: string) => void
  t: (key: LocalCopyKey, values?: Record<string, string | number>) => string
}

const STATUS_ORDER: TaskStatus[] = ['todo', 'in_progress', 'in_review', 'done']

function nextStatuses(status: TaskStatus): TaskStatus[] {
  switch (status) {
    case 'todo':
      return ['todo', 'in_progress']
    case 'in_progress':
      return ['in_progress', 'in_review', 'done']
    case 'in_review':
      return ['in_review', 'in_progress', 'done']
    case 'done':
      return ['done']
  }
}

function formatTaskDate(value: string): string {
  const date = new Date(value)
  return Number.isNaN(date.getTime())
    ? value
    : date.toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' })
}

function statusLabel(status: TaskStatus, t: LocalTasksWorkspaceProps['t']): string {
  switch (status) {
    case 'todo':
      return t('tasksStatusTodo')
    case 'in_progress':
      return t('tasksStatusInProgress')
    case 'in_review':
      return t('tasksStatusInReview')
    case 'done':
      return t('tasksStatusDone')
  }
}

export function LocalTasksWorkspace({
  channels,
  agents,
  activeChannelId,
  onChannelChange,
  t,
}: LocalTasksWorkspaceProps) {
  const [selectedChannelId, setSelectedChannelId] = useState(activeChannelId ?? '')
  const [tasks, setTasks] = useState<Task[]>([])
  const [dependencies, setDependencies] = useState<Record<number, number[]>>({})
  const [selectedTaskNumber, setSelectedTaskNumber] = useState<number | null>(null)
  const [title, setTitle] = useState('')
  const [query, setQuery] = useState('')
  const [view, setView] = useState<'board' | 'list'>('board')
  const [claimAgentId, setClaimAgentId] = useState('')
  const [dependencyNumber, setDependencyNumber] = useState('')
  const [progressDraft, setProgressDraft] = useState('')
  const [loading, setLoading] = useState(false)
  const [action, setAction] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [dragTaskNumber, setDragTaskNumber] = useState<number | null>(null)
  const [dragStatus, setDragStatus] = useState<TaskStatus | null>(null)

  const channelOptions = useMemo(
    () => channels.map((channel) => ({ value: channel.id, label: `# ${channel.name}` })),
    [channels],
  )
  const channelAgents = useMemo(
    () => agents.filter((agent) => agent.channel_id === selectedChannelId),
    [agents, selectedChannelId],
  )
  const agentOptions = useMemo(
    () => channelAgents.map((agent) => ({
      value: agent.id,
      label: agent.name,
      meta: agent.status,
    })),
    [channelAgents],
  )
  const selectedTask = useMemo(
    () => tasks.find((task) => task.taskNumber === selectedTaskNumber) ?? null,
    [selectedTaskNumber, tasks],
  )
  const visibleTasks = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase()
    if (!normalized) return tasks
    return tasks.filter((task) => (
      task.title.toLocaleLowerCase().includes(normalized)
      || task.progress?.toLocaleLowerCase().includes(normalized)
      || task.assigneeName?.toLocaleLowerCase().includes(normalized)
      || task.status.includes(normalized)
      || String(task.taskNumber).includes(normalized)
    ))
  }, [query, tasks])
  const dependencyOptions = useMemo(() => {
    if (!selectedTask) return []
    const current = new Set(dependencies[selectedTask.taskNumber] ?? [])
    return tasks
      .filter((task) => task.taskNumber !== selectedTask.taskNumber && !current.has(task.taskNumber))
      .map((task) => ({
        value: String(task.taskNumber),
        label: `#${task.taskNumber} ${task.title}`,
        meta: statusLabel(task.status, t),
      }))
  }, [dependencies, selectedTask, t, tasks])

  const loadTaskSnapshot = useCallback(async (channelId: string) => {
    const nextTasks = await localApi.listTasks(channelId)
    const dependencyEntries = await Promise.all(nextTasks.map(async (task) => {
      const response = await localApi.getTaskDependencies(channelId, task.taskNumber)
      return [task.taskNumber, response.dependsOn] as const
    }))
    return {
      tasks: nextTasks,
      dependencies: Object.fromEntries(dependencyEntries) as Record<number, number[]>,
    }
  }, [])

  useEffect(() => {
    const fallback = channels[0]?.id ?? ''
    const next = activeChannelId && channels.some((channel) => channel.id === activeChannelId)
      ? activeChannelId
      : channels.some((channel) => channel.id === selectedChannelId)
        ? selectedChannelId
        : fallback
    if (next !== selectedChannelId) setSelectedChannelId(next)
  }, [activeChannelId, channels, selectedChannelId])

  useEffect(() => {
    setClaimAgentId((current) => (
      channelAgents.some((agent) => agent.id === current)
        ? current
        : channelAgents[0]?.id ?? ''
    ))
  }, [channelAgents])

  useEffect(() => {
    setProgressDraft(selectedTask?.progress ?? '')
    setDependencyNumber('')
  }, [selectedTask])

  useEffect(() => {
    let cancelled = false
    if (!selectedChannelId) {
      setTasks([])
      setDependencies({})
      setSelectedTaskNumber(null)
      return
    }
    setLoading(true)
    setError(null)
    loadTaskSnapshot(selectedChannelId)
      .then((snapshot) => {
        if (cancelled) return
        setTasks(snapshot.tasks)
        setDependencies(snapshot.dependencies)
        setSelectedTaskNumber((current) => (
          snapshot.tasks.some((task) => task.taskNumber === current) ? current : null
        ))
      })
      .catch((nextError: unknown) => {
        if (!cancelled) {
          setError(nextError instanceof Error ? nextError.message : t('tasksLoadError'))
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [loadTaskSnapshot, selectedChannelId, t])

  const applyTask = useCallback((updated: Task) => {
    setTasks((current) => current.map((task) => task.id === updated.id ? updated : task))
  }, [])

  const runAction = useCallback(async (key: string, task: () => Promise<void>) => {
    setAction(key)
    setError(null)
    try {
      await task()
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : t('tasksActionError'))
    } finally {
      setAction(null)
    }
  }, [t])

  function selectChannel(channelId: string) {
    setSelectedChannelId(channelId)
    onChannelChange(channelId)
    setSelectedTaskNumber(null)
    setQuery('')
  }

  async function createTask(event: FormEvent) {
    event.preventDefault()
    const nextTitle = title.trim()
    if (!selectedChannelId || !nextTitle) return
    await runAction('create', async () => {
      const created = await localApi.createTask(selectedChannelId, nextTitle)
      setTasks((current) => [...current, created])
      setDependencies((current) => ({ ...current, [created.taskNumber]: [] }))
      setSelectedTaskNumber(created.taskNumber)
      setTitle('')
    })
  }

  function refreshTasks() {
    if (!selectedChannelId) return
    void runAction('refresh', async () => {
      const snapshot = await loadTaskSnapshot(selectedChannelId)
      setTasks(snapshot.tasks)
      setDependencies(snapshot.dependencies)
    })
  }

  function updateStatus(task: Task, status: TaskStatus, progress = task.progress ?? '') {
    if (!selectedChannelId || status === task.status && progress === (task.progress ?? '')) return
    void runAction(`status:${task.taskNumber}`, async () => {
      const updated = await localApi.updateTaskStatus(
        selectedChannelId,
        task.taskNumber,
        status,
        progress.trim() || undefined,
      )
      applyTask(updated)
      setProgressDraft(updated.progress ?? '')
    })
  }

  function claimTask() {
    if (!selectedChannelId || !selectedTask || !claimAgentId) return
    void runAction(`claim:${selectedTask.taskNumber}`, async () => {
      applyTask(await localApi.claimTask(
        selectedChannelId,
        selectedTask.taskNumber,
        claimAgentId,
      ))
    })
  }

  function unclaimTask() {
    if (!selectedChannelId || !selectedTask) return
    void runAction(`unclaim:${selectedTask.taskNumber}`, async () => {
      applyTask(await localApi.unclaimTask(selectedChannelId, selectedTask.taskNumber))
    })
  }

  function addDependency() {
    if (!selectedChannelId || !selectedTask || !dependencyNumber) return
    void runAction(`dependency:${selectedTask.taskNumber}`, async () => {
      const response = await localApi.addTaskDependency(
        selectedChannelId,
        selectedTask.taskNumber,
        Number(dependencyNumber),
      )
      setDependencies((current) => ({
        ...current,
        [selectedTask.taskNumber]: response.dependsOn,
      }))
      setDependencyNumber('')
    })
  }

  function removeDependency(dependsOn: number) {
    if (!selectedChannelId || !selectedTask) return
    void runAction(`remove-dependency:${dependsOn}`, async () => {
      const response = await localApi.removeTaskDependency(
        selectedChannelId,
        selectedTask.taskNumber,
        dependsOn,
      )
      setDependencies((current) => ({
        ...current,
        [selectedTask.taskNumber]: response.dependsOn,
      }))
    })
  }

  function dragTask(event: DragEvent, task: Task) {
    setDragTaskNumber(task.taskNumber)
    event.dataTransfer.effectAllowed = 'move'
    event.dataTransfer.setData('text/plain', String(task.taskNumber))
  }

  function dropTask(status: TaskStatus) {
    const task = tasks.find((candidate) => candidate.taskNumber === dragTaskNumber)
    setDragTaskNumber(null)
    setDragStatus(null)
    if (!task || !nextStatuses(task.status).includes(status)) return
    updateStatus(task, status)
  }

  const selectedDependencies = selectedTask
    ? dependencies[selectedTask.taskNumber] ?? []
    : []

  return (
    <section className="local-tasks-workspace" aria-label={t('tasksWorkspace')}>
      <header className="workspace-heading">
        <div>
          <span className="eyebrow">{t('tasksEyebrow')}</span>
          <h1>{t('tasksTitle')}</h1>
          <p>{t('tasksDescription')}</p>
        </div>
        <button
          type="button"
          className="icon-action"
          onClick={refreshTasks}
          disabled={!selectedChannelId || action === 'refresh'}
        >
          <RefreshCw size={15} aria-hidden="true" />
          {t('refresh')}
        </button>
      </header>

      {error && (
        <div className="workspace-error" role="alert">
          <span>{error}</span>
          <button type="button" onClick={() => setError(null)}>{t('dismiss')}</button>
        </div>
      )}

      <div className={`tasks-workspace-body${selectedTask ? ' has-detail' : ''}`}>
        <section className="tasks-board-pane">
          <div className="tasks-toolbar">
            <div className="tasks-channel-control">
              <label htmlFor="tasks-channel">{t('tasksChannel')}</label>
              <LocalSelect
                id="tasks-channel"
                ariaLabel={t('tasksChannel')}
                value={selectedChannelId}
                options={channelOptions}
                onChange={selectChannel}
                disabled={channels.length === 0}
                placeholder={t('tasksSelectChannel')}
              />
            </div>
            <label className="tasks-search" htmlFor="tasks-search">
              <span>{t('tasksFilter')}</span>
              <input
                id="tasks-search"
                type="search"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder={t('tasksFilterPlaceholder')}
                disabled={!selectedChannelId}
              />
            </label>
            <div className="tasks-view-toggle" aria-label={t('tasksView')}>
              <button
                type="button"
                className={view === 'board' ? 'active' : ''}
                aria-pressed={view === 'board'}
                onClick={() => setView('board')}
              >
                <Columns3 size={14} aria-hidden="true" />
                {t('tasksBoardView')}
              </button>
              <button
                type="button"
                className={view === 'list' ? 'active' : ''}
                aria-pressed={view === 'list'}
                onClick={() => setView('list')}
              >
                <List size={14} aria-hidden="true" />
                {t('tasksListView')}
              </button>
            </div>
          </div>

          <form className="task-create-form" onSubmit={createTask}>
            <label htmlFor="task-title">{t('tasksNewTask')}</label>
            <div>
              <input
                id="task-title"
                value={title}
                onChange={(event) => setTitle(event.target.value)}
                placeholder={t('tasksTitlePlaceholder')}
                disabled={!selectedChannelId || action === 'create'}
              />
              <button
                type="submit"
                className="primary-action"
                disabled={!selectedChannelId || !title.trim() || action === 'create'}
              >
                <Plus size={14} aria-hidden="true" />
                {action === 'create' ? t('tasksCreating') : t('tasksCreate')}
              </button>
            </div>
          </form>

          {!selectedChannelId && (
            <div className="workspace-empty">
              <span>01</span>
              <h3>{t('tasksNoChannel')}</h3>
              <p>{t('tasksNoChannelDescription')}</p>
            </div>
          )}

          {selectedChannelId && loading && tasks.length === 0 && (
            <p className="quiet-copy">{t('tasksLoading')}</p>
          )}

          {selectedChannelId && !loading && tasks.length === 0 && (
            <div className="workspace-empty">
              <span>02</span>
              <h3>{t('tasksEmpty')}</h3>
              <p>{t('tasksEmptyDescription')}</p>
            </div>
          )}

          {tasks.length > 0 && visibleTasks.length === 0 && (
            <div className="workspace-empty">
              <span>03</span>
              <h3>{t('tasksNoMatches')}</h3>
              <p>{t('tasksNoMatchesDescription')}</p>
            </div>
          )}

          {view === 'board' && visibleTasks.length > 0 && (
            <div className="task-board-scroll">
              <div className="task-board">
                {STATUS_ORDER.map((status) => {
                  const columnTasks = visibleTasks.filter((task) => task.status === status)
                  return (
                    <section
                      className={`task-column status-${status}${dragStatus === status ? ' drag-over' : ''}`}
                      key={status}
                      onDragOver={(event) => {
                        event.preventDefault()
                        setDragStatus(status)
                      }}
                      onDragLeave={() => setDragStatus(null)}
                      onDrop={() => dropTask(status)}
                    >
                      <header>
                        <span className="task-status-dot" aria-hidden="true" />
                        <h2>{statusLabel(status, t)}</h2>
                        <strong>{columnTasks.length}</strong>
                      </header>
                      <div>
                        {columnTasks.length === 0 && (
                          <p className="task-column-empty">{t('tasksNoStatusTasks')}</p>
                        )}
                        {columnTasks.map((task) => {
                          const deps = dependencies[task.taskNumber] ?? []
                          const blocking = deps.filter((number) => (
                            tasks.find((candidate) => candidate.taskNumber === number)?.status !== 'done'
                          ))
                          return (
                            <button
                              type="button"
                              className={`task-card${selectedTaskNumber === task.taskNumber ? ' selected' : ''}`}
                              key={task.id}
                              draggable
                              onDragStart={(event) => dragTask(event, task)}
                              onDragEnd={() => {
                                setDragTaskNumber(null)
                                setDragStatus(null)
                              }}
                              onClick={() => setSelectedTaskNumber(task.taskNumber)}
                            >
                              <span className="task-number">#{task.taskNumber}</span>
                              <strong>{task.title}</strong>
                              {task.progress && <p>{task.progress}</p>}
                              <footer>
                                <span>{task.assigneeName ? `@${task.assigneeName}` : t('tasksUnassigned')}</span>
                                {blocking.length > 0 && (
                                  <span className="task-blocked">
                                    <Link2 size={11} aria-hidden="true" />
                                    {t('tasksBlockedBy', { tasks: blocking.join(', #') })}
                                  </span>
                                )}
                              </footer>
                            </button>
                          )
                        })}
                      </div>
                    </section>
                  )
                })}
              </div>
            </div>
          )}

          {view === 'list' && visibleTasks.length > 0 && (
            <div className="task-list">
              {visibleTasks.map((task) => (
                <button
                  type="button"
                  className={selectedTaskNumber === task.taskNumber ? 'selected' : ''}
                  key={task.id}
                  onClick={() => setSelectedTaskNumber(task.taskNumber)}
                >
                  <span className="task-number">#{task.taskNumber}</span>
                  <span className={`task-status status-${task.status}`}>
                    {statusLabel(task.status, t)}
                  </span>
                  <strong>{task.title}</strong>
                  <small>{task.assigneeName ? `@${task.assigneeName}` : t('tasksUnassigned')}</small>
                </button>
              ))}
            </div>
          )}
        </section>

        {selectedTask && (
          <aside className="task-detail-pane" aria-label={t('tasksDetail')}>
            <header>
              <div>
                <span className="eyebrow">{t('tasksDetail')}</span>
                <h2>#{selectedTask.taskNumber} {selectedTask.title}</h2>
              </div>
              <button
                type="button"
                aria-label={t('close')}
                onClick={() => setSelectedTaskNumber(null)}
              >
                <X size={15} aria-hidden="true" />
              </button>
            </header>

            <section>
              <h3>{t('tasksStatus')}</h3>
              <div className="task-status-actions">
                {nextStatuses(selectedTask.status).map((status) => (
                  <button
                    type="button"
                    className={status === selectedTask.status ? 'active' : ''}
                    key={status}
                    onClick={() => updateStatus(selectedTask, status, progressDraft)}
                    disabled={
                      status === selectedTask.status
                      || action === `status:${selectedTask.taskNumber}`
                    }
                  >
                    {statusLabel(status, t)}
                  </button>
                ))}
              </div>
            </section>

            <section>
              <h3>{t('tasksAssignee')}</h3>
              {selectedTask.assigneeName ? (
                <div className="task-current-assignee">
                  <span>@{selectedTask.assigneeName}</span>
                  <button
                    type="button"
                    onClick={unclaimTask}
                    disabled={action === `unclaim:${selectedTask.taskNumber}`}
                  >
                    <UserMinus size={13} aria-hidden="true" />
                    {t('tasksUnclaim')}
                  </button>
                </div>
              ) : (
                <div className="task-claim-control">
                  <LocalSelect
                    id="task-assignee"
                    ariaLabel={t('tasksAssignee')}
                    value={claimAgentId}
                    options={agentOptions}
                    onChange={setClaimAgentId}
                    disabled={channelAgents.length === 0}
                    placeholder={t('tasksSelectAgent')}
                  />
                  <button
                    type="button"
                    onClick={claimTask}
                    disabled={!claimAgentId || action === `claim:${selectedTask.taskNumber}`}
                  >
                    <UserPlus size={13} aria-hidden="true" />
                    {t('tasksClaim')}
                  </button>
                </div>
              )}
              {channelAgents.length === 0 && (
                <p className="quiet-copy">{t('tasksNoAgents')}</p>
              )}
            </section>

            <section>
              <h3>{t('tasksProgress')}</h3>
              <textarea
                aria-label={t('tasksProgress')}
                value={progressDraft}
                onChange={(event) => setProgressDraft(event.target.value)}
                placeholder={t('tasksProgressPlaceholder')}
              />
              <button
                type="button"
                onClick={() => updateStatus(selectedTask, selectedTask.status, progressDraft)}
                disabled={
                  progressDraft === (selectedTask.progress ?? '')
                  || action === `status:${selectedTask.taskNumber}`
                }
              >
                {t('tasksSaveProgress')}
              </button>
            </section>

            <section>
              <h3>{t('tasksDependencies')}</h3>
              {selectedDependencies.length > 0 && (
                <div className="task-dependency-list">
                  {selectedDependencies.map((number) => {
                    const dependency = tasks.find((task) => task.taskNumber === number)
                    return (
                      <div key={number}>
                        <span>
                          #{number} {dependency?.title ?? t('tasksUnknownDependency')}
                        </span>
                        <button
                          type="button"
                          aria-label={t('tasksRemoveDependency', { task: number })}
                          onClick={() => removeDependency(number)}
                          disabled={action === `remove-dependency:${number}`}
                        >
                          <X size={12} aria-hidden="true" />
                        </button>
                      </div>
                    )
                  })}
                </div>
              )}
              <div className="task-dependency-control">
                <LocalSelect
                  id="task-dependency"
                  ariaLabel={t('tasksDependencies')}
                  value={dependencyNumber}
                  options={dependencyOptions}
                  onChange={setDependencyNumber}
                  disabled={dependencyOptions.length === 0}
                  placeholder={t('tasksSelectDependency')}
                />
                <button
                  type="button"
                  onClick={addDependency}
                  disabled={
                    !dependencyNumber
                    || action === `dependency:${selectedTask.taskNumber}`
                  }
                >
                  <Link2 size={13} aria-hidden="true" />
                  {t('tasksAddDependency')}
                </button>
              </div>
            </section>

            <footer>
              <span>{t('tasksCreated')}</span>
              <time dateTime={selectedTask.createdAt}>{formatTaskDate(selectedTask.createdAt)}</time>
            </footer>
          </aside>
        )}
      </div>
    </section>
  )
}

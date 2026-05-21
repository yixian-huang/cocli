import { useEffect, useState, useCallback, type DragEvent } from 'react'
import { useTaskStore } from '@/stores/taskStore'
import { useChannelStore } from '@/stores/channelStore'
import { tasks as tasksApi } from '@/api/client'
import { toastError, toast } from '@/stores/toastStore'
import { cn } from '@/lib/utils'
import { contextAutoForkModeVariant, parseContextAutoForkDetail } from '@/lib/contextAutoForkDetail'
import { ListTodo, Plus, X, LayoutList, Columns3, Link, RefreshCw, ChevronRight, ChevronDown } from 'lucide-react'
import type { Task, TaskExecutionTimeline, TaskExecutionRunTimeline, ExecutionIntentStatus, ExecutionRunStatus } from '@/lib/types'
import { Button, Badge } from '@/components/ui'
import { ExecutionTimelineSkeleton, TaskBoardSkeleton } from '@/components/Skeleton'
import { taskStatusVariant, taskStatusLabel } from '@/lib/status'

const EMPTY_TASKS: Task[] = []

const COLUMNS = [
  { key: 'todo', label: 'Todo', color: 'bg-surface-tertiary' },
  { key: 'in_progress', label: 'In Progress', color: 'bg-info/35' },
  { key: 'in_review', label: 'In Review', color: 'bg-warning/40' },
  { key: 'done', label: 'Done', color: 'bg-success/35' },
]

function DependencyBadges({ deps, tasks }: { deps: number[]; tasks: Task[] }) {
  if (deps.length === 0) return null
  const blocking = deps.filter((d) => {
    const t = tasks.find((t) => t.taskNumber === d)
    return t && t.status !== 'done'
  })
  if (blocking.length === 0) return null
  return (
    <div className="flex flex-wrap gap-1 mt-1">
      {blocking.map((d) => (
        <Badge key={d} variant="error" size="sm" className="gap-0.5">
          <Link className="h-2.5 w-2.5" />
          blocked by #{d}
        </Badge>
      ))}
    </div>
  )
}

function TaskCard({ task, deps, allTasks, onSelect, onDragStart }: {
  task: Task
  deps: number[]
  allTasks: Task[]
  onSelect: () => void
  onDragStart: (e: DragEvent) => void
}) {
  return (
    <div
      draggable
      onDragStart={onDragStart}
      onClick={onSelect}
      className="cursor-pointer rounded-lg border border-border-default bg-surface-primary p-2 text-xs text-content-primary transition-shadow hover:shadow-sm"
    >
      <div className="flex items-center gap-1.5 mb-1">
        <span className="text-muted-foreground font-mono">#{task.taskNumber}</span>
        {task.assigneeName && (
          <span className="ml-auto text-muted-foreground truncate max-w-[60px]">@{task.assigneeName}</span>
        )}
      </div>
      <div className="line-clamp-2 text-foreground">{task.title}</div>
      {task.progress && (
        <div className="mt-1 text-muted-foreground truncate">{task.progress}</div>
      )}
      <DependencyBadges deps={deps} tasks={allTasks} />
    </div>
  )
}

function executionIntentVariant(status?: ExecutionIntentStatus) {
  switch (status) {
    case 'running':
      return 'info' as const
    case 'completed':
      return 'success' as const
    case 'failed':
      return 'error' as const
    case 'canceled':
      return 'warning' as const
    default:
      return 'default' as const
  }
}

function executionRunVariant(status?: ExecutionRunStatus) {
  switch (status) {
    case 'running':
      return 'info' as const
    case 'succeeded':
      return 'success' as const
    case 'failed':
      return 'error' as const
    case 'canceled':
      return 'warning' as const
    default:
      return 'default' as const
  }
}

function fmtDateTime(value?: string) {
  if (!value) return '—'
  const t = new Date(value)
  if (Number.isNaN(t.getTime())) return value
  return t.toLocaleString()
}

function TimelineRun({
  run,
  expanded,
  onToggle,
}: {
  run: TaskExecutionRunTimeline
  expanded: boolean
  onToggle: () => void
}) {
  return (
    <div className="rounded-md border p-2 space-y-1">
      <div className="flex items-center gap-1.5">
        <button type="button" onClick={onToggle} className="text-muted-foreground hover:text-foreground">
          {expanded ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
        </button>
        <span className="font-mono text-[11px]">run {run.id.slice(0, 8)}</span>
        <Badge size="sm" variant={executionRunVariant(run.status)}>
          {run.status}
        </Badge>
        <span className="text-[11px] text-muted-foreground ml-auto">{fmtDateTime(run.startedAt)}</span>
      </div>
      {run.agentSession && (
        <div className="text-[11px] text-muted-foreground pl-5">
          session {run.agentSession.sessionId || run.agentSession.id.slice(0, 8)}
        </div>
      )}
      {expanded && (
        <div className="pl-5 space-y-1">
          {run.summary && <p className="text-[11px] whitespace-pre-wrap">{run.summary}</p>}
          {run.activities && run.activities.length > 0 ? (
            <div className="space-y-1 border-l pl-2">
              {run.activities.slice(0, 8).map((activity) => {
                const parsed = parseContextAutoForkDetail(activity.detail)
                return (
                  <div key={activity.id} className="text-[11px]">
                    <span className="text-muted-foreground">{fmtDateTime(activity.createdAt)}</span>
                    <span className="mx-1 text-muted-foreground">·</span>
                    <span className="font-medium">{activity.activity}</span>
                    {activity.detail ? (
                      <span className="text-muted-foreground inline-flex flex-wrap items-center gap-1.5">
                        <span> — {parsed.text}</span>
                        {parsed.mode && (
                          <Badge
                            variant={contextAutoForkModeVariant(parsed.mode)}
                            size="sm"
                            className="normal-case"
                          >
                            {parsed.mode}
                          </Badge>
                        )}
                      </span>
                    ) : null}
                  </div>
                )
              })}
              {run.activities.length > 8 && (
                <div className="text-[11px] text-muted-foreground">
                  +{run.activities.length - 8} more activity entries
                </div>
              )}
            </div>
          ) : (
            <div className="text-[11px] text-muted-foreground">No activity entries</div>
          )}
        </div>
      )}
    </div>
  )
}

function ExecutionTimelineSection({
  timeline,
  loading,
  error,
  expandedRuns,
  onToggleRun,
  onRefresh,
}: {
  timeline: TaskExecutionTimeline | null
  loading: boolean
  error: string | null
  expandedRuns: Record<string, boolean>
  onToggleRun: (runId: string) => void
  onRefresh: () => void
}) {
  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <span className="text-muted-foreground">Execution</span>
        <Button variant="ghost" size="sm" onClick={onRefresh} className="h-5 px-1.5 text-[11px] gap-1">
          <RefreshCw className={cn('h-3 w-3', loading && 'animate-spin')} />
          refresh
        </Button>
      </div>
      {error ? (
        <div className="text-[11px] text-error">{error}</div>
      ) : loading && !timeline ? (
        <ExecutionTimelineSkeleton />
      ) : !timeline || timeline.intents.length === 0 ? (
        <div className="text-[11px] text-muted-foreground">No execution timeline</div>
      ) : (
        <div className="space-y-2">
          {timeline.intents.map((intent) => (
            <div key={intent.id} className="rounded-md border p-2 space-y-2">
              <div className="flex items-center gap-1.5">
                <span className="font-mono text-[11px]">intent {intent.id.slice(0, 8)}</span>
                <Badge size="sm" variant={executionIntentVariant(intent.status)}>
                  {intent.status}
                </Badge>
                <span className="text-[11px] text-muted-foreground ml-auto">{intent.scope}</span>
              </div>
              <div className="text-[11px] text-muted-foreground">
                created {fmtDateTime(intent.createdAt)} · runs {intent.runs.length}
              </div>
              <div className="space-y-1.5">
                {intent.runs.map((run) => (
                  <TimelineRun
                    key={run.id}
                    run={run}
                    expanded={!!expandedRuns[run.id]}
                    onToggle={() => onToggleRun(run.id)}
                  />
                ))}
                {intent.runs.length === 0 && (
                  <div className="text-[11px] text-muted-foreground">No runs for this intent</div>
                )}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

function TaskDetail({ task, deps, allTasks, onClose }: { task: Task; deps: number[]; allTasks: Task[]; onClose: () => void }) {
  const updateTask = useTaskStore((s) => s.updateTask)
  const [updating, setUpdating] = useState(false)
  const [timeline, setTimeline] = useState<TaskExecutionTimeline | null>(null)
  const [timelineLoading, setTimelineLoading] = useState(false)
  const [timelineError, setTimelineError] = useState<string | null>(null)
  const [expandedRuns, setExpandedRuns] = useState<Record<string, boolean>>({})

  const loadTimeline = useCallback(async () => {
    setTimelineLoading(true)
    setTimelineError(null)
    try {
      const detail = await tasksApi.executionTimeline(task.channelId, task.taskNumber)
      setTimeline(detail)
    } catch (err) {
      setTimelineError(err instanceof Error ? err.message : 'Failed to load execution timeline')
    } finally {
      setTimelineLoading(false)
    }
  }, [task.channelId, task.taskNumber])

  const handleStatusChange = async (newStatus: Task['status']) => {
    if (newStatus === task.status || updating) return
    setUpdating(true)
    try {
      const updated = await tasksApi.updateStatus(task.channelId, task.taskNumber, newStatus)
      updateTask(updated)
      toast('Status updated', 'success')
    } catch (err) {
      toastError(err instanceof Error ? err.message : 'Failed to update')
    } finally {
      setUpdating(false)
    }
  }

  useEffect(() => {
    setExpandedRuns({})
    loadTimeline()
  }, [loadTimeline])

  const blockingDeps = deps.filter((d) => {
    const t = allTasks.find((t) => t.taskNumber === d)
    return t && t.status !== 'done'
  })
  const resolvedDeps = deps.filter((d) => {
    const t = allTasks.find((t) => t.taskNumber === d)
    return t && t.status === 'done'
  })

  const toggleRun = (runId: string) => {
    setExpandedRuns((prev) => ({ ...prev, [runId]: !prev[runId] }))
  }

  return (
    <div className="border-t p-3 space-y-3">
      <div className="flex items-center justify-between">
        <span className="text-xs font-semibold text-muted-foreground">Task #{task.taskNumber}</span>
        <Button variant="ghost" size="sm" onClick={onClose} className="h-5 w-5 p-0">
          <X className="h-3.5 w-3.5" />
        </Button>
      </div>
      <h4 className="text-sm font-medium">{task.title}</h4>
      <div className="space-y-2 text-xs">
        <div className="flex items-center gap-2">
          <span className="text-muted-foreground w-16">Status</span>
          <select
            value={task.status}
            onChange={(e) => handleStatusChange(e.target.value as Task['status'])}
            disabled={updating}
            className={cn(
              'text-xs px-1.5 py-0.5 rounded font-medium border-0 cursor-pointer appearance-none',
              updating && 'opacity-50',
            )}
          >
            {COLUMNS.map((col) => (
              <option key={col.key} value={col.key}>{col.label}</option>
            ))}
          </select>
        </div>
        {task.assigneeName && (
          <div className="flex items-center gap-2">
            <span className="text-muted-foreground w-16">Assignee</span>
            <span>@{task.assigneeName}</span>
          </div>
        )}
        <div className="flex items-start gap-2">
          <span className="text-muted-foreground w-16">Lifecycle</span>
          <div className="flex flex-wrap gap-1.5">
            {task.executionIntentStatus && (
              <Badge size="sm" variant={executionIntentVariant(task.executionIntentStatus)}>
                intent {task.executionIntentStatus}
              </Badge>
            )}
            {task.executionRunStatus && (
              <Badge size="sm" variant={executionRunVariant(task.executionRunStatus)}>
                run {task.executionRunStatus}
              </Badge>
            )}
            {task.executionIntentId && (
              <span className="font-mono text-[11px] text-muted-foreground">
                i:{task.executionIntentId.slice(0, 8)}
              </span>
            )}
            {task.executionRunId && (
              <span className="font-mono text-[11px] text-muted-foreground">
                r:{task.executionRunId.slice(0, 8)}
              </span>
            )}
          </div>
        </div>
        {deps.length > 0 && (
          <div>
            <span className="text-muted-foreground">Dependencies</span>
            <div className="mt-1 flex flex-wrap gap-1">
              {blockingDeps.map((d) => (
                <Badge key={d} variant="error" size="sm" className="gap-0.5">
                  <Link className="h-2.5 w-2.5" />
                  blocked by #{d}
                </Badge>
              ))}
              {resolvedDeps.map((d) => (
                <Badge key={d} variant="success" size="sm" className="line-through">
                  #{d}
                </Badge>
              ))}
            </div>
          </div>
        )}
        {task.progress && (
          <div>
            <span className="text-muted-foreground">Progress</span>
            <p className="mt-1 text-foreground whitespace-pre-wrap">{task.progress}</p>
          </div>
        )}
        <div className="flex items-center gap-2 text-muted-foreground">
          <span className="w-16">Created</span>
          <span>{new Date(task.createdAt).toLocaleDateString()}</span>
        </div>
        <ExecutionTimelineSection
          timeline={timeline}
          loading={timelineLoading}
          error={timelineError}
          expandedRuns={expandedRuns}
          onToggleRun={toggleRun}
          onRefresh={loadTimeline}
        />
      </div>
    </div>
  )
}

export function TaskBoard({ loading: loadingProp }: { loading?: boolean }) {
  const activeId = useChannelStore((s) => s.activeChannelId)
  const tasks = useTaskStore((s) => s.tasksByChannel.get(activeId ?? '') ?? EMPTY_TASKS)
  const storeLoading = useTaskStore((s) => s.loading)
  const fetchTasks = useTaskStore((s) => s.fetchTasks)
  const createTask = useTaskStore((s) => s.createTask)
  const updateTask = useTaskStore((s) => s.updateTask)
  const getDeps = useTaskStore((s) => s.getDeps)
  const [showCreate, setShowCreate] = useState(false)
  const [title, setTitle] = useState('')
  const [creating, setCreating] = useState(false)
  const [selectedTask, setSelectedTask] = useState<Task | null>(null)
  const [view, setView] = useState<'list' | 'board'>('list')
  const [dragOverCol, setDragOverCol] = useState<string | null>(null)
  const [dragTaskNum, setDragTaskNum] = useState<number | null>(null)
  const loading = loadingProp ?? storeLoading

  useEffect(() => {
    if (activeId) fetchTasks(activeId)
  }, [activeId, fetchTasks])

  // Keep selected task in sync
  useEffect(() => {
    if (selectedTask) {
      const fresh = tasks.find((t) => t.taskNumber === selectedTask.taskNumber)
      if (fresh && fresh !== selectedTask) setSelectedTask(fresh)
    }
  }, [tasks, selectedTask])

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!title.trim() || !activeId) return
    setCreating(true)
    try {
      await createTask(activeId, title.trim())
      setTitle('')
      setShowCreate(false)
    } catch (err) {
      toastError(err instanceof Error ? err.message : 'Failed to create task')
    } finally {
      setCreating(false)
    }
  }

  const handleDrop = useCallback(async (status: Task['status']) => {
    setDragOverCol(null)
    if (dragTaskNum == null || !activeId) return
    const task = tasks.find((t) => t.taskNumber === dragTaskNum)
    if (!task || task.status === status) return
    try {
      const updated = await tasksApi.updateStatus(activeId, task.taskNumber, status)
      updateTask(updated)
    } catch (err) {
      toastError(err instanceof Error ? err.message : 'Failed to update')
    }
    setDragTaskNum(null)
  }, [dragTaskNum, activeId, tasks, updateTask])

  if (!activeId) return null

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between px-3 py-2 border-b shrink-0">
        <h3 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">Tasks</h3>
        <div className="flex items-center gap-1">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setView(view === 'list' ? 'board' : 'list')}
            className="h-5 w-5 p-0 text-muted-foreground"
            title={view === 'list' ? 'Board view' : 'List view'}
          >
            {view === 'list' ? <Columns3 className="h-3.5 w-3.5" /> : <LayoutList className="h-3.5 w-3.5" />}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setShowCreate(!showCreate)}
            className="h-5 w-5 p-0 text-muted-foreground"
            title="Create task"
          >
            <Plus className="h-3.5 w-3.5" />
          </Button>
        </div>
      </div>

      {showCreate && (
        <form onSubmit={handleCreate} className="px-3 py-2 border-b shrink-0">
          <input
            type="text"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="Task title..."
            className="w-full rounded border border-border-default bg-surface-primary px-2 py-1 text-xs focus:outline-none focus:ring-1 focus:ring-ring"
            autoFocus
            disabled={creating}
          />
        </form>
      )}

      {loading && tasks.length === 0 ? (
        <TaskBoardSkeleton view={view} />
      ) : tasks.length === 0 && !showCreate ? (
        <div className="p-4 text-center text-sm text-muted-foreground">
          <ListTodo className="h-8 w-8 mx-auto mb-2 opacity-50" />
          No tasks in this channel
        </div>
      ) : view === 'list' ? (
        <div className="flex-1 overflow-y-auto">
          {tasks.map((task) => {
            const deps = activeId ? getDeps(activeId, task.taskNumber) : []
            return (
              <div
                key={task.id}
                onClick={() => setSelectedTask(task)}
                className={cn(
                  'flex flex-col px-3 py-2 text-sm border-b last:border-0 cursor-pointer hover:bg-accent/50 transition-colors',
                  selectedTask?.taskNumber === task.taskNumber && 'bg-accent/30',
                )}
              >
                <div className="flex items-center gap-2">
                  <span className="text-muted-foreground font-mono text-xs">#{task.taskNumber}</span>
                  <Badge variant={taskStatusVariant(task.status)} size="sm">
                    {taskStatusLabel(task.status)}
                  </Badge>
                  <span className="flex-1 truncate text-xs">{task.title}</span>
                </div>
                {task.progress && (
                  <div className="ml-6 mt-1 text-xs text-muted-foreground truncate">{task.progress}</div>
                )}
                <DependencyBadges deps={deps} tasks={tasks} />
              </div>
            )
          })}
        </div>
      ) : (
        <div className="flex-1 overflow-y-auto p-2 space-y-2">
          {COLUMNS.map((col) => {
            const colTasks = tasks.filter((t) => t.status === col.key)
            return (
              <div
                key={col.key}
                onDragOver={(e) => { e.preventDefault(); setDragOverCol(col.key) }}
                onDragLeave={() => setDragOverCol(null)}
                onDrop={() => handleDrop(col.key as Task['status'])}
                className={cn(
                  'rounded-lg p-2 transition-colors',
                  dragOverCol === col.key ? 'bg-accent-secondary ring-1 ring-accent-primary/30' : 'bg-surface-tertiary',
                )}
              >
                <div className="flex items-center gap-1.5 mb-2">
                  <div className={cn('w-2 h-2 rounded-full', col.color)} />
                  <span className="text-xs font-semibold text-muted-foreground">{col.label}</span>
                  <span className="text-xs text-muted-foreground ml-auto">{colTasks.length}</span>
                </div>
                <div className="space-y-1.5">
                  {colTasks.map((task) => (
                    <TaskCard
                      key={task.id}
                      task={task}
                      deps={activeId ? getDeps(activeId, task.taskNumber) : []}
                      allTasks={tasks}
                      onSelect={() => setSelectedTask(task)}
                      onDragStart={() => setDragTaskNum(task.taskNumber)}
                    />
                  ))}
                </div>
              </div>
            )
          })}
        </div>
      )}

      {selectedTask && (
        <TaskDetail
          task={selectedTask}
          deps={activeId ? getDeps(activeId, selectedTask.taskNumber) : []}
          allTasks={tasks}
          onClose={() => setSelectedTask(null)}
        />
      )}
    </div>
  )
}

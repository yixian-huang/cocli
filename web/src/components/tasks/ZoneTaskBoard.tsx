import { useEffect, useMemo, useState } from 'react'
import { useZoneStore } from '@/stores/zoneStore'
import { useZoneTaskBoardStore } from '@/stores/zoneTaskBoardStore'
import { taskStatusLabel, taskStatusVariant } from '@/lib/status'
import { Badge, Button } from '@/components/ui'
import type { Task } from '@/lib/types'
import { ChevronRight, Filter, RefreshCw } from 'lucide-react'

const BOARD_COLUMNS: Task['status'][] = ['pending', 'claimed', 'in_progress', 'completed', 'failed']

function displayAssignee(task: Task) {
  return task.assigneeName || task.assigneeId || 'unassigned'
}

function taskDependsOn(task: Task): string[] {
  const raw = task as Task & { dependsOnTaskIds?: string[]; dependencyTaskIds?: string[] }
  return raw.dependsOnTaskIds || raw.dependencyTaskIds || []
}

export function ZoneTaskBoard() {
  const zoneId = useZoneStore((s) => s.activeZoneId)
  const tasks = useZoneTaskBoardStore((s) => s.tasks)
  const filters = useZoneTaskBoardStore((s) => s.filters)
  const loading = useZoneTaskBoardStore((s) => s.loading)
  const error = useZoneTaskBoardStore((s) => s.error)
  const timelineByTaskId = useZoneTaskBoardStore((s) => s.timelineByTaskId)
  const timelineLoadingByTaskId = useZoneTaskBoardStore((s) => s.timelineLoadingByTaskId)
  const setFilters = useZoneTaskBoardStore((s) => s.setFilters)
  const fetchTasks = useZoneTaskBoardStore((s) => s.fetchTasks)
  const fetchTimeline = useZoneTaskBoardStore((s) => s.fetchTimeline)
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null)

  useEffect(() => {
    if (!zoneId) return
    fetchTasks(zoneId)
  }, [zoneId, fetchTasks, filters])

  const selectedTask = tasks.find((task) => task.id === selectedTaskId) || null
  const selectedTimeline = selectedTaskId ? timelineByTaskId[selectedTaskId] : null
  const selectedTimelineLoading = selectedTaskId ? timelineLoadingByTaskId[selectedTaskId] : false

  useEffect(() => {
    if (!selectedTaskId) return
    if (selectedTimeline) return
    fetchTimeline(selectedTaskId)
  }, [selectedTaskId, selectedTimeline, fetchTimeline])

  const channelOptions = useMemo(() => {
    const channels = new Set<string>()
    for (const task of tasks) {
      if (task.channelId) channels.add(task.channelId)
    }
    return [...channels]
  }, [tasks])

  const assigneeOptions = useMemo(() => {
    const assignees = new Set<string>()
    for (const task of tasks) {
      if (task.assigneeId) assignees.add(task.assigneeId)
    }
    return [...assignees]
  }, [tasks])

  return (
    <div className="flex-1 min-h-0 flex flex-col">
      <div className="h-12 border-b px-4 flex items-center justify-between">
        <div className="text-sm font-semibold">Zone Task Board</div>
        <Button
          variant="ghost"
          size="sm"
          className="gap-1"
          onClick={() => zoneId && fetchTasks(zoneId)}
          disabled={!zoneId || loading}
        >
          <RefreshCw className={`h-3.5 w-3.5 ${loading ? 'animate-spin' : ''}`} />
          Refresh
        </Button>
      </div>

      <div className="border-b px-4 py-3 grid grid-cols-1 md:grid-cols-4 gap-2 text-xs">
        <label className="inline-flex items-center gap-2">
          <Filter className="h-3.5 w-3.5 text-muted-foreground" />
          <select
            className="border rounded px-2 py-1 bg-background w-full"
            value={filters.status || ''}
            onChange={(e) => setFilters({ status: e.target.value || undefined })}
          >
            <option value="">All statuses</option>
            {BOARD_COLUMNS.map((status) => (
              <option key={status} value={status}>
                {taskStatusLabel(status)}
              </option>
            ))}
          </select>
        </label>
        <select
          className="border rounded px-2 py-1 bg-background"
          value={filters.channelId || ''}
          onChange={(e) => setFilters({ channelId: e.target.value || undefined })}
        >
          <option value="">All channels</option>
          {channelOptions.map((channelId) => (
            <option key={channelId} value={channelId}>
              {channelId}
            </option>
          ))}
        </select>
        <select
          className="border rounded px-2 py-1 bg-background"
          value={filters.assignee || ''}
          onChange={(e) => setFilters({ assignee: e.target.value || undefined })}
        >
          <option value="">All assignees</option>
          {assigneeOptions.map((assigneeId) => (
            <option key={assigneeId} value={assigneeId}>
              {assigneeId}
            </option>
          ))}
        </select>
        <input
          className="border rounded px-2 py-1 bg-background"
          placeholder="dependsOnTaskId"
          value={filters.dependency || ''}
          onChange={(e) => setFilters({ dependency: e.target.value || undefined })}
        />
      </div>

      {error && <div className="px-4 py-2 text-xs text-error">{error}</div>}

      <div className="flex-1 min-h-0 flex overflow-hidden">
        <div className="flex-1 min-w-0 overflow-auto p-3">
          <div className="grid grid-cols-1 md:grid-cols-5 gap-3 min-w-[920px]">
            {BOARD_COLUMNS.map((column) => {
              const columnTasks = tasks.filter((task) => task.status === column)
              return (
                <section key={column} className="rounded border bg-muted/20 min-h-[260px]">
                  <header className="px-2.5 py-2 border-b flex items-center gap-2">
                    <Badge size="sm" variant={taskStatusVariant(column)}>{taskStatusLabel(column)}</Badge>
                    <span className="text-xs text-muted-foreground ml-auto">{columnTasks.length}</span>
                  </header>
                  <div className="p-2 space-y-2">
                    {columnTasks.length === 0 ? (
                      <div className="text-[11px] text-muted-foreground px-1 py-2">No tasks</div>
                    ) : (
                      columnTasks.map((task) => {
                        const deps = taskDependsOn(task)
                        return (
                          <button
                            type="button"
                            key={task.id}
                            onClick={() => setSelectedTaskId(task.id)}
                            className={`w-full text-left rounded border bg-background px-2 py-2 hover:bg-accent/40 ${
                              selectedTaskId === task.id ? 'ring-1 ring-primary/40' : ''
                            }`}
                          >
                            <div className="flex items-center gap-1.5">
                              <span className="font-mono text-[11px] text-muted-foreground">#{task.taskNumber}</span>
                              <ChevronRight className="h-3.5 w-3.5 text-muted-foreground ml-auto" />
                            </div>
                            <div className="text-xs mt-1 line-clamp-2">{task.title}</div>
                            <div className="text-[11px] text-muted-foreground mt-1 truncate">
                              {displayAssignee(task)}
                            </div>
                            {deps.length > 0 && (
                              <div className="mt-1 text-[11px] text-muted-foreground line-clamp-1">
                                deps: {deps.join(', ')}
                              </div>
                            )}
                          </button>
                        )
                      })
                    )}
                  </div>
                </section>
              )
            })}
          </div>
        </div>

        <aside className="w-[360px] border-l overflow-y-auto hidden lg:block">
          {!selectedTask ? (
            <div className="p-4 text-sm text-muted-foreground">Select a task to view timeline</div>
          ) : (
            <div className="p-4 space-y-3">
              <div>
                <div className="text-xs text-muted-foreground mb-1">Task #{selectedTask.taskNumber}</div>
                <h4 className="text-sm font-medium">{selectedTask.title}</h4>
                <div className="mt-1 text-xs text-muted-foreground">{displayAssignee(selectedTask)}</div>
              </div>

              <div>
                <div className="text-xs font-semibold text-muted-foreground mb-2">Execution timeline</div>
                {selectedTimelineLoading && (
                  <div className="text-xs text-muted-foreground">Loading timeline...</div>
                )}
                {!selectedTimelineLoading && !selectedTimeline && (
                  <div className="text-xs text-muted-foreground">No timeline data</div>
                )}
                {selectedTimeline && (
                  <div className="space-y-2">
                    {selectedTimeline.intents.map((intent) => (
                      <div key={intent.id} className="rounded border p-2">
                        <div className="flex items-center gap-2 text-xs">
                          <span className="font-mono">{intent.id.slice(0, 8)}</span>
                          <Badge size="sm">{intent.status}</Badge>
                        </div>
                        <div className="text-[11px] text-muted-foreground mt-1">scope: {intent.scope}</div>
                        <div className="mt-2 space-y-1">
                          {intent.runs.map((run) => (
                            <div key={run.id} className="rounded border px-2 py-1 text-[11px]">
                              <div className="flex items-center gap-1.5">
                                <span className="font-mono">{run.id.slice(0, 8)}</span>
                                <Badge size="sm">{run.status}</Badge>
                              </div>
                              {run.summary && (
                                <p className="text-muted-foreground mt-1 whitespace-pre-wrap">{run.summary}</p>
                              )}
                            </div>
                          ))}
                          {intent.runs.length === 0 && (
                            <div className="text-[11px] text-muted-foreground">No runs</div>
                          )}
                        </div>
                      </div>
                    ))}
                    {selectedTimeline.intents.length === 0 && (
                      <div className="text-xs text-muted-foreground">No execution intents</div>
                    )}
                  </div>
                )}
              </div>
            </div>
          )}
        </aside>
      </div>
    </div>
  )
}

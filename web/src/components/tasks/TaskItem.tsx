import type { Task } from '@/lib/types'
import { Badge } from '@/components/ui'
import { taskStatusVariant, taskStatusLabel } from '@/lib/status'

export function TaskItem({ task }: { task: Task }) {
  return (
    <div className="flex flex-col px-3 py-2 text-sm border-b last:border-0">
      <div className="flex items-center gap-2">
        <span className="text-muted-foreground font-mono text-xs">#{task.taskNumber}</span>
        <Badge variant={taskStatusVariant(task.status)} size="sm">
          {taskStatusLabel(task.status)}
        </Badge>
        <span className="flex-1 truncate">{task.title}</span>
        {task.assigneeName && (
          <span className="text-xs text-muted-foreground">@{task.assigneeName}</span>
        )}
      </div>
      {task.progress && (
        <div className="ml-6 mt-1 text-xs text-muted-foreground truncate">{task.progress}</div>
      )}
    </div>
  )
}

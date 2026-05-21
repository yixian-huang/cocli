import type { HTMLAttributes } from 'react'
import { cn } from '@/lib/utils'

export function Skeleton({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return <div className={cn('animate-pulse rounded bg-muted', className)} {...props} />
}

export function MessageSkeleton() {
  return (
    <div className="flex gap-3 px-4 py-3">
      <Skeleton className="h-8 w-8 rounded-full shrink-0" />
      <div className="flex-1 space-y-2">
        <div className="flex gap-2">
          <Skeleton className="h-3 w-20" />
          <Skeleton className="h-3 w-12" />
        </div>
        <Skeleton className="h-3 w-full max-w-[300px]" />
        <Skeleton className="h-3 w-full max-w-[200px]" />
      </div>
    </div>
  )
}

export function MessageListSkeleton({ count = 6 }: { count?: number }) {
  return (
    <div className="flex-1 overflow-y-auto py-1" data-testid="message-list-skeleton">
      {Array.from({ length: count }).map((_, i) => (
        <MessageSkeleton key={i} />
      ))}
    </div>
  )
}

export function ChannelSkeleton() {
  return (
    <div className="flex items-center gap-2 px-3 py-1.5">
      <Skeleton className="h-4 w-4 rounded" />
      <Skeleton className="h-3 w-24" />
    </div>
  )
}

function TaskListItemSkeleton() {
  return (
    <div className="rounded-lg border bg-background px-3 py-2 space-y-2">
      <div className="flex items-center gap-2">
        <Skeleton className="h-3 w-8 rounded" />
        <Skeleton className="h-5 w-[4.5rem] rounded-full" />
        <Skeleton className="ml-auto h-3 w-24 rounded" />
      </div>
      <Skeleton className="h-3 w-full max-w-[280px]" />
      <Skeleton className="h-3 w-full max-w-[220px]" />
    </div>
  )
}

function TaskColumnSkeleton() {
  return (
    <div className="rounded-lg bg-muted/30 p-2">
      <div className="mb-2 flex items-center gap-1.5">
        <Skeleton className="h-2 w-2 rounded-full" />
        <Skeleton className="h-3 w-16 rounded" />
        <Skeleton className="ml-auto h-3 w-5 rounded" />
      </div>
      <div className="space-y-1.5">
        <TaskListItemSkeleton />
        <TaskListItemSkeleton />
      </div>
    </div>
  )
}

export function TaskBoardSkeleton({ view = 'list' }: { view?: 'list' | 'board' }) {
  if (view === 'board') {
    return (
      <div className="flex-1 overflow-y-auto p-2 space-y-2" data-testid="task-board-skeleton">
        {Array.from({ length: 4 }).map((_, i) => (
          <TaskColumnSkeleton key={i} />
        ))}
      </div>
    )
  }

  return (
    <div className="flex-1 overflow-y-auto" data-testid="task-board-skeleton">
      {Array.from({ length: 6 }).map((_, i) => (
        <div key={i} className="border-b px-3 py-2 last:border-0">
          <TaskListItemSkeleton />
        </div>
      ))}
    </div>
  )
}

function TurnCardSkeleton() {
  return (
    <div className="rounded border border-border bg-card p-3 space-y-3">
      <div className="flex items-center gap-2">
        <Skeleton className="h-3.5 w-3.5 rounded" />
        <Skeleton className="h-3 w-16 rounded" />
        <Skeleton className="h-3 w-20 rounded" />
        <Skeleton className="ml-auto h-3 w-24 rounded" />
      </div>
      <div className="space-y-2 border-t pt-3">
        <Skeleton className="h-3 w-full max-w-[260px]" />
        <Skeleton className="h-3 w-full max-w-[220px]" />
        <Skeleton className="h-3 w-full max-w-[180px]" />
      </div>
    </div>
  )
}

function FlowTurnSkeleton() {
  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2">
        <div className="h-px flex-1 bg-border" />
        <Skeleton className="h-3 w-40 rounded" />
        <div className="h-px flex-1 bg-border" />
      </div>
      <div className="space-y-2">
        <Skeleton className="h-3 w-full max-w-[260px]" />
        <Skeleton className="h-3 w-full max-w-[220px]" />
        <Skeleton className="h-3 w-full max-w-[180px]" />
      </div>
    </div>
  )
}

export function TurnLogSkeleton({ viewMode = 'timeline' }: { viewMode?: 'timeline' | 'flow' }) {
  return (
    <div className="flex-1 min-h-0 overflow-y-auto p-3" data-testid="turn-log-skeleton">
      <div className="space-y-3">
        {Array.from({ length: 3 }).map((_, i) =>
          viewMode === 'flow' ? <FlowTurnSkeleton key={i} /> : <TurnCardSkeleton key={i} />
        )}
      </div>
    </div>
  )
}

export function ExecutionTimelineSkeleton() {
  return (
    <div className="space-y-2" data-testid="execution-timeline-skeleton">
      {Array.from({ length: 2 }).map((_, i) => (
        <div key={i} className="rounded-md border p-2 space-y-2">
          <div className="flex items-center gap-2">
            <Skeleton className="h-3 w-24 rounded" />
            <Skeleton className="h-5 w-16 rounded-full" />
            <Skeleton className="ml-auto h-3 w-24 rounded" />
          </div>
          <Skeleton className="h-3 w-full max-w-[220px]" />
          <div className="space-y-1.5 border-l pl-2">
            <Skeleton className="h-3 w-full max-w-[240px]" />
            <Skeleton className="h-3 w-full max-w-[200px]" />
            <Skeleton className="h-3 w-full max-w-[170px]" />
          </div>
        </div>
      ))}
    </div>
  )
}

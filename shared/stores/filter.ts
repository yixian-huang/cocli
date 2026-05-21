// shared/stores/filter.ts
// Filter logic shared between the tasks zustand store and mobile screens.
// The source of truth lives here; mobile/src/lib/tasks/filterReducer.ts
// re-exports from this module.
import type { TaskStatus, Task } from '@shared/types'

export const FILTER_VALUES = [
  'all',
  'pending',
  'claimed',
  'in_progress',
  'completed',
  'failed',
] as const

export type FilterValue = (typeof FILTER_VALUES)[number]

export const FILTER_LABELS: Record<FilterValue, string> = {
  all: 'All',
  pending: 'Pending',
  claimed: 'Claimed',
  in_progress: 'In Progress',
  completed: 'Done',
  failed: 'Failed',
}

/** Convert a filter value to the `status` query param for zoneTasks.list. */
export function filterToStatus(f: FilterValue): TaskStatus | undefined {
  if (f === 'all') return undefined
  return f as TaskStatus
}

/** Predicate for client-side filtering when refresh comes back. */
export function matchesFilter(t: Task, f: FilterValue): boolean {
  if (f === 'all') return true
  return t.status === f
}

/** Sort tasks for display: newest updated first. */
export function sortForDisplay(rows: Task[]): Task[] {
  return [...rows].sort((a, b) => (a.updatedAt < b.updatedAt ? 1 : -1))
}

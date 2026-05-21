// shared/stores/tasks.ts
import type { Task, TaskStatus } from '@shared/types'
import { sortForDisplay, type FilterValue, filterToStatus } from './filter'

export interface TasksState {
  rows: Task[]
  loading: boolean
  error: string | null
  filter: FilterValue
  setFilter: (f: FilterValue) => void
  /** Replace one row in-place (used after optimistic mutation). */
  upsertOne: (t: Task) => void
  /** Remove one row by id (used after delete). */
  removeOne: (id: string) => void
}

/** Minimal store factory — consumers wire zustand in their own layer. */
export function makeInitialState(): Omit<TasksState, 'setFilter' | 'upsertOne' | 'removeOne'> {
  return { rows: [], loading: false, error: null, filter: 'all' }
}

/** Filter rows client-side by the active filter value. */
export function applyFilter(rows: Task[], filter: FilterValue): Task[] {
  const status: TaskStatus | undefined = filterToStatus(filter)
  if (!status) return rows
  return rows.filter((t) => t.status === status)
}

export { sortForDisplay }

// shared/stores/tasks.ts
import { create } from 'zustand'
import { zoneTasks } from '@shared/api/client'
import type { Task } from '@shared/types'
import { sortForDisplay, type FilterValue, filterToStatus } from './filter'

export interface TasksState {
  rows: Task[]
  loading: boolean
  error: string | null
  filter: FilterValue
  setFilter: (f: FilterValue) => void
  /** Fetch from server. Pure side-effect. */
  refresh: (zoneId: string) => Promise<void>
  /** Replace one row in-place (used after optimistic mutation). */
  upsertOne: (t: Task) => void
  /** Remove one row by id (used after delete). */
  removeOne: (id: string) => void
}

export const useTasksStore = create<TasksState>((set, get) => ({
  rows: [],
  loading: false,
  error: null,
  filter: 'all',

  setFilter: (f) => set({ filter: f }),

  refresh: async (zoneId) => {
    set({ loading: true, error: null })
    try {
      const status = filterToStatus(get().filter)
      const rows = await zoneTasks.list(zoneId, status ? { status } : undefined)
      set({ rows: sortForDisplay(rows), loading: false })
    } catch (e) {
      set({ loading: false, error: (e as Error).message })
    }
  },

  upsertOne: (t) => {
    const rows = get().rows
    const idx = rows.findIndex((r) => r.id === t.id)
    const next = idx >= 0 ? [...rows.slice(0, idx), t, ...rows.slice(idx + 1)] : [t, ...rows]
    set({ rows: sortForDisplay(next) })
  },

  removeOne: (id) => set({ rows: get().rows.filter((r) => r.id !== id) }),
}))

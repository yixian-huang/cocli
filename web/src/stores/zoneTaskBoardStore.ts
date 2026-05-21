import { create } from 'zustand'
import { zoneTasks as zoneTasksApi } from '@/api/client'
import type { Task, TaskExecutionTimeline } from '@/lib/types'

interface ZoneTaskFilters {
  status?: string
  channelId?: string
  assignee?: string
  dependency?: string
}

interface ZoneTaskBoardState {
  tasks: Task[]
  filters: ZoneTaskFilters
  loading: boolean
  error: string | null
  timelineByTaskId: Record<string, TaskExecutionTimeline>
  timelineLoadingByTaskId: Record<string, boolean>
  setFilters: (filters: Partial<ZoneTaskFilters>) => void
  fetchTasks: (zoneId: string) => Promise<void>
  fetchTimeline: (taskId: string) => Promise<void>
}

export const useZoneTaskBoardStore = create<ZoneTaskBoardState>((set, get) => ({
  tasks: [],
  filters: {},
  loading: false,
  error: null,
  timelineByTaskId: {},
  timelineLoadingByTaskId: {},

  setFilters: (filters) =>
    set((state) => ({
      filters: { ...state.filters, ...filters },
    })),

  fetchTasks: async (zoneId) => {
    set({ loading: true, error: null })
    try {
      const { filters } = get()
      const tasks = await zoneTasksApi.list(zoneId, filters)
      set({
        tasks: Array.isArray(tasks) ? tasks : [],
        loading: false,
      })
    } catch (err) {
      set({
        loading: false,
        error: err instanceof Error ? err.message : 'Failed to load zone tasks',
      })
    }
  },

  fetchTimeline: async (taskId) => {
    set((state) => ({
      timelineLoadingByTaskId: { ...state.timelineLoadingByTaskId, [taskId]: true },
    }))
    try {
      const timeline = await zoneTasksApi.timeline(taskId)
      set((state) => ({
        timelineByTaskId: { ...state.timelineByTaskId, [taskId]: timeline },
        timelineLoadingByTaskId: { ...state.timelineLoadingByTaskId, [taskId]: false },
      }))
    } catch {
      set((state) => ({
        timelineLoadingByTaskId: { ...state.timelineLoadingByTaskId, [taskId]: false },
      }))
    }
  },
}))

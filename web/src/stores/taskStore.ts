import { create } from 'zustand'
import type { Task } from '@/lib/types'
import { tasks as tasksApi } from '@/api/client'

interface TaskState {
  tasksByChannel: Map<string, Task[]>
  // Map of "channelId:taskNumber" -> array of task numbers it depends on
  dependencies: Map<string, number[]>
  loading: boolean
  fetchTasks: (channelId: string) => Promise<void>
  fetchDependencies: (channelId: string, taskNumbers: number[]) => Promise<void>
  updateTask: (task: Task) => void
  createTask: (channelId: string, title: string) => Promise<void>
  getDeps: (channelId: string, taskNumber: number) => number[]
}

export const useTaskStore = create<TaskState>((set, get) => ({
  tasksByChannel: new Map(),
  dependencies: new Map(),
  loading: false,

  fetchTasks: async (channelId) => {
    set({ loading: true })
    try {
      const tasks = await tasksApi.list(channelId)
      const taskList = Array.isArray(tasks) ? tasks : []
      set((s) => {
        const map = new Map(s.tasksByChannel)
        map.set(channelId, taskList)
        const deps = new Map(s.dependencies)
        for (const key of deps.keys()) {
          if (key.startsWith(`${channelId}:`)) deps.delete(key)
        }
        return { tasksByChannel: map, dependencies: deps, loading: false }
      })
      // Fetch dependencies for all tasks
      const taskNumbers = taskList.map((t) => t.taskNumber)
      if (taskNumbers.length > 0) {
        get().fetchDependencies(channelId, taskNumbers)
      }
    } catch {
      set({ loading: false })
    }
  },

  fetchDependencies: async (channelId, taskNumbers) => {
    const results = await Promise.allSettled(
      taskNumbers.map((num) => tasksApi.getDependencies(channelId, num))
    )
    set((s) => {
      const deps = new Map(s.dependencies)
      results.forEach((result, i) => {
        const key = `${channelId}:${taskNumbers[i]}`
        deps.delete(key)
        if (result.status !== 'fulfilled') return
        const dependsOn = Array.isArray(result.value.dependsOn) ? result.value.dependsOn : []
        if (dependsOn.length > 0) deps.set(key, dependsOn)
      })
      return { dependencies: deps }
    })
  },

  updateTask: (task) =>
    set((s) => {
      const map = new Map(s.tasksByChannel)
      const existing = map.get(task.channelId) || []
      const idx = existing.findIndex((t) => t.id === task.id)
      if (idx >= 0) {
        const updated = [...existing]
        updated[idx] = task
        map.set(task.channelId, updated)
      } else {
        map.set(task.channelId, [...existing, task])
      }
      return { tasksByChannel: map }
    }),

  createTask: async (channelId, title) => {
    const task = await tasksApi.create(channelId, title)
    get().updateTask(task)
  },

  getDeps: (channelId, taskNumber) => {
    return get().dependencies.get(`${channelId}:${taskNumber}`) || []
  },
}))

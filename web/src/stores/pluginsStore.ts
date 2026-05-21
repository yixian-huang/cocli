import { create } from 'zustand'
import { storageKey } from '@shared/brand'
import type { Plugin, PluginCapability, PluginRegistration } from '@shared/types'

interface PluginsState {
  plugins: Plugin[]
  init: () => void
  list: () => Promise<Plugin[]>
  register: (name: string, capabilities: PluginCapability[]) => Promise<PluginRegistration>
  revoke: (id: string) => Promise<void>
}

const KEY = 'cocli-plugins'

function load(): Plugin[] {
  const raw = localStorage.getItem(storageKey(KEY))
  if (!raw) return []
  try {
    return JSON.parse(raw) as Plugin[]
  } catch {
    return []
  }
}

function persist(plugins: Plugin[]) {
  localStorage.setItem(storageKey(KEY), JSON.stringify(plugins))
}

export const usePluginsStore = create<PluginsState>((set, get) => ({
  plugins: [],

  init: () => {
    set({ plugins: load() })
  },

  list: async () => {
    const items = load()
    set({ plugins: items })
    return items
  },

  register: async (name, capabilities) => {
    const plugin: Plugin = {
      id: crypto.randomUUID(),
      name,
      capabilities,
      createdAt: new Date().toISOString(),
      lastSeenAt: null,
    }
    const token = crypto.randomUUID()
    const next = [...get().plugins, plugin]
    set({ plugins: next })
    persist(next)
    return { plugin, token }
  },

  revoke: async (id) => {
    const next = get().plugins.filter((p) => p.id !== id)
    set({ plugins: next })
    persist(next)
  },
}))

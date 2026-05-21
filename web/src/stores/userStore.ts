import { create } from 'zustand'
import type { User } from '@/lib/types'
import { users as usersApi, auth as authApi, setApiKey, getApiKey } from '@/api/client'
import { useZoneStore } from '@/stores/zoneStore'
import { storageKey } from '@/brand'

interface UserState {
  user: User | null
  allUsers: User[]
  apiKey: string
  loading: boolean
  login: (key: string) => Promise<void>
  loginWithPassword: (username: string, password: string) => Promise<void>
  signup: (code: string, username: string, email: string, password: string) => Promise<void>
  logout: () => void
  init: () => Promise<void>
  setUser: (user: User) => void
  fetchUsers: () => Promise<void>
}

export const isAdmin = () => useUserStore.getState().user?.role === 'admin'

export const useUserStore = create<UserState>((set) => ({
  user: null,
  allUsers: [],
  apiKey: getApiKey(),
  loading: true,

  login: async (key: string) => {
    setApiKey(key)
    set({ apiKey: key })
    const user = await usersApi.me()
    set({ user, loading: false })
  },

  loginWithPassword: async (username: string, password: string) => {
    const data = await authApi.login(username, password)
    setApiKey(data.apiKey)
    set({ user: data.user, apiKey: data.apiKey, loading: false })
    const zoneId = useZoneStore.getState().activeZoneId
    if (zoneId) {
      const allUsers = await usersApi.list(zoneId)
      set({ allUsers })
    }
  },

  signup: async (code: string, username: string, email: string, password: string) => {
    const data = await authApi.signup(code, username, email, password)
    setApiKey(data.apiKey)
    set({ user: data.user, apiKey: data.apiKey, loading: false })
    const zoneId = useZoneStore.getState().activeZoneId
    if (zoneId) {
      const allUsers = await usersApi.list(zoneId)
      set({ allUsers })
    }
  },

  logout: () => {
    localStorage.removeItem(storageKey('api-key'))
    set({ user: null, apiKey: '', loading: false })
  },

  setUser: (user) => set({ user }),

  fetchUsers: async () => {
    const zoneId = useZoneStore.getState().activeZoneId
    if (!zoneId) return
    try {
      const allUsers = await usersApi.list(zoneId)
      set({ allUsers: allUsers || [] })
    } catch {
      // ignore
    }
  },

  init: async () => {
    const key = getApiKey()
    if (!key) {
      set({ loading: false })
      return
    }
    try {
      const user = await usersApi.me()
      set({ user, apiKey: key, loading: false })
    } catch {
      set({ loading: false })
    }
  },
}))

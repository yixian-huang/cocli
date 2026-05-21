import { create } from 'zustand'
import { chatrsCredentials as api } from '@/api/client'
import type { CreateCredentialInput, TenantProviderKey } from '@/lib/types'

interface CocliCredentialsState {
  // Keyed by zoneId so switching zones doesn't blow away the cache.
  byZone: Record<string, TenantProviderKey[]>
  loadingByZone: Record<string, boolean>
  errorByZone: Record<string, string | null>

  fetch: (zoneId: string) => Promise<void>
  create: (zoneId: string, input: CreateCredentialInput) => Promise<TenantProviderKey>
  remove: (zoneId: string, name: string) => Promise<void>
  clearError: (zoneId: string) => void
}

export const useCocliCredentialsStore = create<CocliCredentialsState>((set) => ({
  byZone: {},
  loadingByZone: {},
  errorByZone: {},

  fetch: async (zoneId) => {
    set((s) => ({
      loadingByZone: { ...s.loadingByZone, [zoneId]: true },
      errorByZone: { ...s.errorByZone, [zoneId]: null },
    }))
    try {
      const list = await api.list(zoneId)
      set((s) => ({
        byZone: { ...s.byZone, [zoneId]: Array.isArray(list) ? list : [] },
        loadingByZone: { ...s.loadingByZone, [zoneId]: false },
      }))
    } catch (err) {
      set((s) => ({
        loadingByZone: { ...s.loadingByZone, [zoneId]: false },
        errorByZone: {
          ...s.errorByZone,
          [zoneId]: err instanceof Error ? err.message : 'failed to load credentials',
        },
      }))
    }
  },

  create: async (zoneId, input) => {
    const created = await api.create(zoneId, input)
    set((s) => ({
      byZone: {
        ...s.byZone,
        [zoneId]: [created, ...(s.byZone[zoneId] ?? [])],
      },
    }))
    return created
  },

  remove: async (zoneId, name) => {
    await api.remove(zoneId, name)
    set((s) => ({
      byZone: {
        ...s.byZone,
        [zoneId]: (s.byZone[zoneId] ?? []).filter((k) => k.name !== name),
      },
    }))
  },

  clearError: (zoneId) => {
    set((s) => ({ errorByZone: { ...s.errorByZone, [zoneId]: null } }))
  },
}))

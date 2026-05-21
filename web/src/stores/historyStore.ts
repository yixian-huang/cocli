import { create } from 'zustand'
import { history as historyApi } from '@/api/client'
import type { HistoryMessage, HistoryQuery } from '@/lib/types'

const DEFAULT_PAGE_SIZE = 30

interface HistoryState {
  items: HistoryMessage[]
  loading: boolean
  error: string | null
  page: number
  pageSize: number
  total: number
  filters: Omit<HistoryQuery, 'page' | 'pageSize'>
  setFilters: (filters: Partial<Omit<HistoryQuery, 'page' | 'pageSize'>>) => void
  setPage: (page: number) => void
  setPageSize: (pageSize: number) => void
  fetch: (zoneId: string) => Promise<void>
  reset: () => void
}

export const useHistoryStore = create<HistoryState>((set, get) => ({
  items: [],
  loading: false,
  error: null,
  page: 1,
  pageSize: DEFAULT_PAGE_SIZE,
  total: 0,
  filters: {},

  setFilters: (filters) =>
    set((state) => ({
      filters: { ...state.filters, ...filters },
      page: 1,
    })),

  setPage: (page) => set({ page }),
  setPageSize: (pageSize) => set({ pageSize, page: 1 }),

  fetch: async (zoneId) => {
    set({ loading: true, error: null })
    try {
      const { page, pageSize, filters } = get()
      const res = await historyApi.list(zoneId, { ...filters, page, pageSize })
      set({
        items: Array.isArray(res.items) ? res.items : [],
        total: res.total ?? 0,
        page: res.page ?? page,
        pageSize: res.pageSize ?? pageSize,
        loading: false,
      })
    } catch (err) {
      set({
        loading: false,
        error: err instanceof Error ? err.message : 'Failed to load history',
      })
    }
  },

  reset: () =>
    set({
      items: [],
      loading: false,
      error: null,
      page: 1,
      pageSize: DEFAULT_PAGE_SIZE,
      total: 0,
      filters: {},
    }),
}))

import { create } from 'zustand'
import type { BookmarkEntry } from '@/lib/types'
import { bookmarks as bookmarksApi } from '@/api/client'

interface BookmarkState {
  bookmarkedIds: Set<string>
  bookmarks: BookmarkEntry[]
  loading: boolean
  fetchBookmarks: () => Promise<void>
  toggleBookmark: (messageId: string) => Promise<void>
  isBookmarked: (messageId: string) => boolean
  addOptimistic: (messageId: string) => void
  removeOptimistic: (messageId: string) => void
}

export const useBookmarkStore = create<BookmarkState>((set, get) => ({
  bookmarkedIds: new Set(),
  bookmarks: [],
  loading: false,

  fetchBookmarks: async () => {
    set({ loading: true })
    try {
      const data = await bookmarksApi.list()
      const ids = new Set(data.bookmarks.map((b) => b.message.id))
      set({ bookmarks: data.bookmarks, bookmarkedIds: ids, loading: false })
    } catch {
      set({ loading: false })
    }
  },

  toggleBookmark: async (messageId) => {
    const isCurrently = get().isBookmarked(messageId)
    if (isCurrently) {
      get().removeOptimistic(messageId)
      try {
        await bookmarksApi.remove(messageId)
      } catch {
        get().addOptimistic(messageId)
      }
    } else {
      get().addOptimistic(messageId)
      try {
        await bookmarksApi.create(messageId)
        // Refetch to get full bookmark entry with channel name
        await get().fetchBookmarks()
      } catch {
        get().removeOptimistic(messageId)
      }
    }
  },

  isBookmarked: (messageId) => get().bookmarkedIds.has(messageId),

  addOptimistic: (messageId) =>
    set((state) => ({
      bookmarkedIds: new Set([...state.bookmarkedIds, messageId]),
    })),

  removeOptimistic: (messageId) =>
    set((state) => ({
      bookmarkedIds: new Set([...state.bookmarkedIds].filter((id) => id !== messageId)),
      bookmarks: state.bookmarks.filter((b) => b.message.id !== messageId),
    })),
}))

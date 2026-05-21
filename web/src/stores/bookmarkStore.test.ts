import { describe, it, expect, beforeEach } from 'vitest'
import { useBookmarkStore } from './bookmarkStore'
import type { BookmarkEntry } from '@/lib/types'

const makeEntry = (id: string, messageId: string): BookmarkEntry => ({
  bookmarkId: id,
  message: {
    id: messageId,
    channelId: 'ch1',
    senderType: 'user',
    senderName: 'alice',
    content: `Message ${messageId}`,
    seq: 1,
    createdAt: '2026-04-07T00:00:00Z',
  },
  channelName: 'general',
  createdAt: '2026-04-07T00:00:00Z',
})

describe('bookmarkStore', () => {
  beforeEach(() => {
    useBookmarkStore.setState({
      bookmarkedIds: new Set(),
      bookmarks: [],
      loading: false,
    })
  })

  it('starts with empty bookmarks', () => {
    expect(useBookmarkStore.getState().bookmarks).toEqual([])
    expect(useBookmarkStore.getState().bookmarkedIds.size).toBe(0)
  })

  it('isBookmarked returns false for unknown message', () => {
    expect(useBookmarkStore.getState().isBookmarked('unknown')).toBe(false)
  })

  it('isBookmarked returns true after adding', () => {
    useBookmarkStore.setState({
      bookmarkedIds: new Set(['msg1']),
      bookmarks: [makeEntry('b1', 'msg1')],
    })
    expect(useBookmarkStore.getState().isBookmarked('msg1')).toBe(true)
  })

  it('addOptimistic adds to bookmarkedIds', () => {
    useBookmarkStore.getState().addOptimistic('msg1')
    expect(useBookmarkStore.getState().isBookmarked('msg1')).toBe(true)
  })

  it('removeOptimistic removes from bookmarkedIds and bookmarks', () => {
    useBookmarkStore.setState({
      bookmarkedIds: new Set(['msg1']),
      bookmarks: [makeEntry('b1', 'msg1')],
    })
    useBookmarkStore.getState().removeOptimistic('msg1')
    expect(useBookmarkStore.getState().isBookmarked('msg1')).toBe(false)
    expect(useBookmarkStore.getState().bookmarks).toHaveLength(0)
  })
})

import { useNavigate } from 'react-router-dom'
import { useBookmarkStore } from '@/stores/bookmarkStore'
import { useZoneStore } from '@/stores/zoneStore'
import { messagePath } from '@/lib/paths'
import { X } from 'lucide-react'
import { SectionHeader } from '@/components/ui'

export function SavedMessages() {
  const navigate = useNavigate()
  const zoneSlug = useZoneStore((s) => s.activeZoneSlug)
  const bookmarks = useBookmarkStore((s) => s.bookmarks)
  const toggleBookmark = useBookmarkStore((s) => s.toggleBookmark)

  const handleClick = (entry: (typeof bookmarks)[0]) => {
    navigate(messagePath({ zoneSlug, channelId: entry.message.channelId, messageId: entry.message.id }))
  }

  if (bookmarks.length === 0) return null

  return (
    <div className="px-2 py-2">
      <SectionHeader title="Saved" />
      {bookmarks.map((entry) => (
        <div
          key={entry.bookmarkId}
          className="group flex items-center gap-2 w-full px-2 py-1.5 rounded text-sm hover:bg-accent transition-colors cursor-pointer"
          onClick={() => handleClick(entry)}
        >
          <span className="truncate flex-1 text-foreground">
            {entry.message.content.slice(0, 40)}
            {entry.message.content.length > 40 ? '...' : ''}
          </span>
          <span className="text-[10px] text-muted-foreground shrink-0">
            #{entry.channelName}
          </span>
          <button
            onClick={(e) => {
              e.stopPropagation()
              toggleBookmark(entry.message.id)
            }}
            className="opacity-0 group-hover:opacity-100 shrink-0 p-0.5 rounded hover:bg-accent-foreground/10 transition-opacity"
            title="Remove bookmark"
          >
            <X className="h-3 w-3 text-muted-foreground" />
          </button>
        </div>
      ))}
    </div>
  )
}

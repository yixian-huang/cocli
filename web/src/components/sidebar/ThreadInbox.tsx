import { useEffect, useMemo } from 'react'
import { useNavigate } from 'react-router-dom'
import { useThreadInboxStore } from '@/stores/threadInboxStore'
import { useThreadStore } from '@/stores/threadStore'
import { useZoneStore } from '@/stores/zoneStore'
import { channelPath } from '@/lib/paths'
import { relativeTime } from '@/lib/utils'
import { Check, RotateCcw } from 'lucide-react'
import { CollapsibleSection, ContextMenuTrigger } from '@/components/ui'
import type { MenuEntry } from '@/components/ui'

export function ThreadInbox({ query }: { query?: string }) {
  const navigate = useNavigate()
  const zoneSlug = useZoneStore((s) => s.activeZoneSlug)
  const fetchThreads = useThreadInboxStore((s) => s.fetchThreads)
  const threads = useThreadInboxStore((s) => s.threads)
  const toggleDone = useThreadInboxStore((s) => s.toggleDone)
  const openThread = useThreadStore((s) => s.openThread)

  const visibleThreads = useMemo(() => {
    const filtered = threads.filter((t) => !t.done)
    const text = (query ?? '').trim().toLowerCase()
    const matched = text
      ? filtered.filter((t) => t.parentMessage.content.toLowerCase().includes(text))
      : filtered
    return [...matched].sort(
      (a, b) => new Date(b.lastActivityAt).getTime() - new Date(a.lastActivityAt).getTime(),
    )
  }, [threads, query])

  useEffect(() => {
    fetchThreads()
  }, [fetchThreads])

  const handleClick = (thread: (typeof visibleThreads)[0]) => {
    navigate(channelPath({ zoneSlug, channelId: thread.parentMessage.channelId }))
    openThread(thread.parentMessage.channelId, thread.parentMessage)
  }

  return (
    <CollapsibleSection
      id="sidebar.threads"
      title="Threads"
      count={threads.length}
    >
      <div className="px-1 pb-2">
        {visibleThreads.map((thread) => {
          const items: MenuEntry[] = [
            {
              id: 'done',
              label: thread.done ? 'Mark as undone' : 'Mark as done',
              onSelect: () => toggleDone(thread.id),
            },
          ]
          return (
            <ContextMenuTrigger key={thread.id} items={items}>
              <div
                className={`group flex items-center gap-2 w-full px-2 py-1.5 rounded text-sm hover:bg-accent transition-colors cursor-pointer ${
                  thread.done ? 'opacity-50' : ''
                }`}
                onClick={() => handleClick(thread)}
              >
                <span className="truncate flex-1 text-foreground">
                  {thread.parentMessage.content.slice(0, 40)}
                  {thread.parentMessage.content.length > 40 ? '...' : ''}
                </span>
                <span className="text-[10px] text-muted-foreground shrink-0">
                  {relativeTime(thread.lastActivityAt)}
                </span>
                <button
                  onClick={(e) => {
                    e.stopPropagation()
                    toggleDone(thread.id)
                  }}
                  className="opacity-0 group-hover:opacity-100 shrink-0 p-0.5 rounded hover:bg-accent-foreground/10 transition-opacity"
                  title={thread.done ? 'Mark undone' : 'Mark done'}
                >
                  {thread.done ? (
                    <RotateCcw className="h-3 w-3 text-muted-foreground" />
                  ) : (
                    <Check className="h-3 w-3 text-muted-foreground" />
                  )}
                </button>
              </div>
            </ContextMenuTrigger>
          )
        })}
        {visibleThreads.length === 0 && (
          <p className="px-2 text-xs text-muted-foreground">No threads</p>
        )}
      </div>
    </CollapsibleSection>
  )
}

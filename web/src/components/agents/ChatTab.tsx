import { useState, useEffect, useCallback } from 'react'
import { dm } from '@/api/client'
import { useZoneStore } from '@/stores/zoneStore'
import { useThreadStore } from '@/stores/threadStore'
import { MessageList } from '@/components/chat/MessageList'
import { MessageInput } from '@/components/chat/MessageInput'
import { ThreadPanel } from '@/components/chat/ThreadPanel'
import { Loader2, Search, X } from 'lucide-react'
import { Button } from '@/components/ui'
import type { Message } from '@/lib/types'

export function ChatTab({ agentName }: { agentName: string }) {
  const [dmChannelId, setDmChannelId] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)
  const [searchOpen, setSearchOpen] = useState(false)
  const [searchQuery, setSearchQuery] = useState('')

  const threadChannel = useThreadStore((s) => s.threadChannel)
  const openThread = useThreadStore((s) => s.openThread)
  const closeThread = useThreadStore((s) => s.closeThread)

  useEffect(() => {
    const zoneId = useZoneStore.getState().activeZoneId
    if (!zoneId) { setLoading(false); return }
    setLoading(true)
    dm.createOrGet(zoneId, agentName, 'agent')
      .then((ch) => setDmChannelId(ch.id))
      .catch(() => setDmChannelId(null))
      .finally(() => setLoading(false))
  }, [agentName])

  // Close thread when agent changes
  useEffect(() => {
    closeThread()
    setSearchOpen(false)
    setSearchQuery('')
  }, [agentName, closeThread])

  const handleReply = useCallback(
    (message: Message) => {
      if (dmChannelId) openThread(dmChannelId, message)
    },
    [dmChannelId, openThread],
  )

  if (loading) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
      </div>
    )
  }

  if (!dmChannelId) {
    return (
      <div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
        Failed to open DM with @{agentName}
      </div>
    )
  }

  return (
    <div className="flex-1 flex flex-col min-h-0">
      {/* Search bar */}
      <div className="shrink-0 flex items-center gap-2 px-3 py-1.5 border-b">
        {searchOpen ? (
          <div className="flex items-center gap-1 flex-1">
            <Search className="h-3.5 w-3.5 text-muted-foreground shrink-0" />
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Search messages..."
              autoFocus
              className="flex-1 bg-transparent text-xs focus:outline-none"
            />
            <Button variant="ghost" size="sm" onClick={() => { setSearchOpen(false); setSearchQuery('') }}>
              <X className="h-3.5 w-3.5" />
            </Button>
          </div>
        ) : (
          <div className="flex items-center gap-2 ml-auto">
            <Button variant="ghost" size="sm" onClick={() => setSearchOpen(true)} title="Search messages">
              <Search className="h-3.5 w-3.5" />
            </Button>
          </div>
        )}
      </div>

      {/* Chat + Thread layout */}
      <div className="flex-1 flex min-h-0 overflow-hidden">
        <div className="flex-1 flex flex-col min-w-0 min-h-0">
          <MessageList key={dmChannelId} channelId={dmChannelId} onReply={handleReply} searchQuery={searchQuery} />
          <MessageInput channelId={dmChannelId} />
        </div>
        {threadChannel && (
          <div className="hidden sm:block">
            <ThreadPanel />
          </div>
        )}
      </div>
    </div>
  )
}

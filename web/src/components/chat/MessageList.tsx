import { useState, useEffect, useLayoutEffect, useRef, useCallback, useMemo } from 'react'
import { useNavigate } from 'react-router-dom'
import { useVirtualizer } from '@tanstack/react-virtual'
import { useMessageStore, EMPTY_MESSAGES } from '@/stores/messageStore'
import { useChannelStore } from '@/stores/channelStore'
import { useViewStore } from '@/stores/viewStore'
import { useAgentStore } from '@/stores/agentStore'
import { useZoneStore } from '@/stores/zoneStore'
import { useThreadInboxStore } from '@/stores/threadInboxStore'
import { messages as messagesApi, search as searchApi } from '@/api/client'
import { MessageItem } from './MessageItem'
import { MessageListSkeleton } from '@/components/Skeleton'
import { ArrowDown, Hash, Loader2, MessageCircle, MessageSquare, Search } from 'lucide-react'
import type { Message } from '@/lib/types'
import { EmptyState } from '@/components/ui'
import { cn } from '@/lib/utils'
import { agentPath } from '@/lib/paths'

interface Props {
  channelId?: string
  onReply?: (message: Message) => void
  searchQuery?: string
  loading?: boolean
}

export function MessageList({ channelId, onReply, searchQuery, loading: loadingProp }: Props) {
  const navigate = useNavigate()
  const setQuotedMessage = useViewStore((s) => s.setQuotedMessage)
  const setActiveAgent = useViewStore((s) => s.setActiveAgent)
  const zoneSlug = useZoneStore((s) => s.activeZoneSlug)

  // Stable agent names — only recomputes when agent names change, not status
  const agentNamesKey = useAgentStore((s) => s.agents.map((a) => a.name).join(','))
  const agentNames = useMemo(() => new Set(agentNamesKey.split(',')), [agentNamesKey])

  const handleAgentClick = useCallback((agentId: string) => {
    setActiveAgent(agentId)
    navigate(agentPath({ zoneSlug, agentId }))
  }, [navigate, setActiveAgent, zoneSlug])

  // Thread summary map: parentMessageId → { replyCount, lastActivityAt }
  const allThreads = useThreadInboxStore((s) => s.threads)
  const threadByMessageId = useMemo(() => {
    const map = new Map<string, { replyCount: number; lastActivityAt: string }>()
    for (const t of allThreads) {
      if (t.parentMessage?.id) {
        map.set(t.parentMessage.id, { replyCount: t.replyCount, lastActivityAt: t.lastActivityAt })
      }
    }
    return map
  }, [allThreads])

  const channels = useChannelStore((s) => s.channels)
  const dmChannels = useChannelStore((s) => s.dmChannels)
  const setActiveChannel = useChannelStore((s) => s.setActiveChannel)
  const storeId = useChannelStore((s) => s.activeChannelId)
  const activeId = channelId ?? storeId
  const activeChannel = useChannelStore((s) =>
    s.channels.find((c) => c.id === activeId) ?? s.dmChannels.find((c) => c.id === activeId),
  )
  const clearUnread = useChannelStore((s) => s.clearUnread)
  const messages = useMessageStore((s) => s.messagesByChannel.get(activeId ?? '') ?? EMPTY_MESSAGES)
  const hasMore = useMessageStore((s) => (activeId ? s.hasMore.get(activeId) ?? false : false))
  const fetchMessages = useMessageStore((s) => s.fetchMessages)
  const loadOlder = useMessageStore((s) => s.loadOlder)
  const parentRef = useRef<HTMLDivElement>(null)
  const prevLenRef = useRef(0)
  const lastMarkedSeqRef = useRef<number>(0)
  const loadingOlderRef = useRef(false)
  const scrollAnchorRef = useRef<{ prevScrollOffset: number; prevScrollHeight: number } | null>(null)
  const isInitialLoadRef = useRef(true)
  const [showNewMsg, setShowNewMsg] = useState(false)
  const [loadingOlder, setLoadingOlder] = useState(false)

  const [searchResults, setSearchResults] = useState<Message[]>([])
  const [searching, setSearching] = useState(false)
  const [searchError, setSearchError] = useState<string | null>(null)
  const [loadError, setLoadError] = useState(false)
  const [selectedIndex, setSelectedIndex] = useState(-1)
  const searchReqSeqRef = useRef(0)

  const isSearching = !!searchQuery?.trim()
  const filteredMessages = isSearching ? searchResults : messages

  const searchResultsView = useMemo(() => {
    if (!isSearching || searchResults.length === 0) return null
    return searchResults.map((msg) => {
      const ch = [...channels, ...dmChannels].find((c) => c.id === msg.channelId)
      return { msg, chName: ch?.name || 'unknown' }
    })
  }, [searchResults, channels, dmChannels, isSearching])

  // Build grouped flags in parallel with message list
  const groupedFlags = useMemo(() => {
    return filteredMessages.map((msg, i) => {
      const prev = filteredMessages[i - 1]
      const isSystem = msg.messageType === 'system' || msg.senderType === 'system'
      return !isSystem && !!prev
        && prev.senderName === msg.senderName
        && prev.senderType === msg.senderType
        && prev.messageType !== 'system' && prev.senderType !== 'system'
        && new Date(msg.createdAt).getTime() - new Date(prev.createdAt).getTime() < 5 * 60 * 1000
    })
  }, [filteredMessages])

  const virtualizer = useVirtualizer({
    count: filteredMessages.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 60,
    overscan: 15,
  })

  useEffect(() => {
    setSelectedIndex(-1)
  }, [activeId, isSearching])

  useEffect(() => {
    if (filteredMessages.length === 0) {
      setSelectedIndex(-1)
      return
    }
    setSelectedIndex((current) => (current < 0 ? current : Math.min(current, filteredMessages.length - 1)))
  }, [filteredMessages.length])

  useEffect(() => {
    const handleNavigate = (event: Event) => {
      if (isSearching || filteredMessages.length === 0) return

      const detail = (event as CustomEvent<{ direction?: 'next' | 'previous' }>).detail
      const direction = detail?.direction
      if (!direction) return

      setSelectedIndex((current) => {
        if (current < 0) {
          const newestIndex = filteredMessages.length - 1
          virtualizer.scrollToIndex(newestIndex, { align: 'center' })
          return newestIndex
        }

        const nextIndex = direction === 'next'
          ? Math.min(current + 1, filteredMessages.length - 1)
          : Math.max(current - 1, 0)

        virtualizer.scrollToIndex(nextIndex, { align: 'center' })
        return nextIndex
      })
    }

    window.addEventListener('message-list:navigate', handleNavigate as EventListener)
    return () => window.removeEventListener('message-list:navigate', handleNavigate as EventListener)
  }, [filteredMessages.length, isSearching, virtualizer])

  // Mark messages as read
  const markRead = useCallback(
    (channelId: string, seq: number) => {
      if (seq > 0 && seq > lastMarkedSeqRef.current) {
        lastMarkedSeqRef.current = seq
        clearUnread(channelId)
        messagesApi.markRead(channelId, seq).catch((err) => console.warn('[api] mark read failed:', err))
      }
    },
    [clearUnread],
  )

  // Fetch messages when channel changes
  useEffect(() => {
    if (activeId) {
      lastMarkedSeqRef.current = 0
      ;(async () => {
        try {
          await fetchMessages(activeId)
          setLoadError(false)
        } catch {
          setLoadError(true)
        }
      })()
    }
  }, [activeId, fetchMessages])

  // Mark read when messages load or new messages arrive while at bottom
  useEffect(() => {
    if (!activeId || messages.length === 0) return
    const container = parentRef.current
    const wasAtBottom =
      !container || container.scrollHeight - container.scrollTop - container.clientHeight < 100
    if (wasAtBottom) {
      const maxSeq = Math.max(...messages.map((m) => m.seq))
      markRead(activeId, maxSeq)
    }
  }, [activeId, messages, markRead])

  // Auto-scroll on new messages (only if already near bottom), else show indicator
  useEffect(() => {
    const container = parentRef.current
    if (!container) return
    const wasAtBottom =
      container.scrollHeight - container.scrollTop - container.clientHeight < 100
    if (messages.length > prevLenRef.current) {
      if (wasAtBottom) {
        virtualizer.scrollToIndex(messages.length - 1, { align: 'end' })
        setShowNewMsg(false)
      } else {
        setShowNewMsg(true)
      }
    }
    prevLenRef.current = messages.length
  }, [messages.length, virtualizer])

  // Set initial load flag on channel switch
  useEffect(() => {
    isInitialLoadRef.current = true
    setShowNewMsg(false)
  }, [activeId])

  // Scroll to bottom once messages are loaded for a new channel
  useEffect(() => {
    if (isInitialLoadRef.current && messages.length > 0) {
      isInitialLoadRef.current = false
      virtualizer.scrollToIndex(messages.length - 1, { align: 'end' })
    }
  }, [messages.length, virtualizer])

  // Scroll-preserving loadOlder
  const handleLoadOlder = useCallback(async () => {
    const container = parentRef.current
    if (!container || !activeId || loadingOlderRef.current) return
    loadingOlderRef.current = true
    setLoadingOlder(true)
    scrollAnchorRef.current = {
      prevScrollOffset: container.scrollTop,
      prevScrollHeight: container.scrollHeight,
    }
    await loadOlder(activeId)
  }, [activeId, loadOlder])

  // Restore scroll position after older messages are prepended
  useLayoutEffect(() => {
    const container = parentRef.current
    const anchor = scrollAnchorRef.current
    if (container && anchor && loadingOlderRef.current) {
      // Adjust scroll by the height delta so the user stays at the same visual position
      const heightDelta = container.scrollHeight - anchor.prevScrollHeight
      container.scrollTop = anchor.prevScrollOffset + heightDelta
      scrollAnchorRef.current = null
      loadingOlderRef.current = false
      setLoadingOlder(false)
    }
  }, [messages])

  // Cache maxSeq to avoid O(n) spread on every scroll
  const maxSeqRef = useRef(0)
  useEffect(() => {
    if (messages.length > 0) {
      let max = 0
      for (const m of messages) { if (m.seq > max) max = m.seq }
      maxSeqRef.current = max
    }
  }, [messages])

  // Load older on scroll to top, mark read on scroll to bottom
  const messagesRef = useRef(messages)
  messagesRef.current = messages

  const handleScroll = useCallback(() => {
    const container = parentRef.current
    if (!container || !activeId) return
    if (hasMore && container.scrollTop < 50) {
      handleLoadOlder()
    }
    const atBottom = container.scrollHeight - container.scrollTop - container.clientHeight < 100
    if (atBottom) setShowNewMsg(false)
    if (atBottom && messagesRef.current.length > 0) {
      markRead(activeId, maxSeqRef.current)
    }
  }, [activeId, hasMore, handleLoadOlder, markRead])

  // Scroll-to-message listener (e.g. from search result click)
  useEffect(() => {
    const handler = (e: Event) => {
      const { msgId } = (e as CustomEvent).detail
      const idx = filteredMessages.findIndex((m) => m.id === msgId)
      if (idx >= 0) {
        setSelectedIndex(idx)
        virtualizer.scrollToIndex(idx, { align: 'center' })
      }
    }
    window.addEventListener('scroll-to-message', handler)
    return () => window.removeEventListener('scroll-to-message', handler)
  }, [filteredMessages, virtualizer])

  const storeLoading = useMessageStore((s) => s.loading)
  const loading = loadingProp ?? storeLoading

  useEffect(() => {
    const query = searchQuery?.trim()
    if (!query) {
      searchReqSeqRef.current++
      setSearchResults([])
      setSearchError(null)
      setSearching(false)
      return
    }
    const reqSeq = ++searchReqSeqRef.current
    const controller = new AbortController()
    const timer = setTimeout(async () => {
      setSearching(true)
      setSearchError(null)
      try {
        const zoneId = useZoneStore.getState().activeZoneId
        if (!zoneId) return
        const res = await searchApi.messages(zoneId, query, undefined, { signal: controller.signal })
        if (searchReqSeqRef.current !== reqSeq) return
        setSearchResults(res.messages || [])
      } catch {
        if (controller.signal.aborted || searchReqSeqRef.current !== reqSeq) return
        setSearchResults([])
        setSearchError('Search failed. Try again.')
      } finally {
        if (searchReqSeqRef.current === reqSeq) {
          setSearching(false)
        }
      }
    }, 300)
    return () => {
      clearTimeout(timer)
      controller.abort()
    }
  }, [searchQuery])

  const scrollToBottom = () => {
    virtualizer.scrollToIndex(filteredMessages.length - 1, { align: 'end' })
    setShowNewMsg(false)
  }

  if (!activeId) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center text-muted-foreground gap-2">
        <MessageSquare className="h-10 w-10 opacity-30" />
        <span className="text-sm">Select a channel to start chatting</span>
      </div>
    )
  }

  if (loadError && messages.length === 0) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center py-12 gap-3">
        <p className="text-sm text-muted-foreground">Failed to load messages</p>
        <button onClick={() => fetchMessages(activeId)} className="text-xs text-primary hover:underline">
          Retry
        </button>
      </div>
    )
  }

  if (loading && messages.length === 0) {
    return <MessageListSkeleton />
  }

  if (!loading && messages.length === 0) {
    const isDM = activeChannel?.type === 'dm'
    const icon = isDM
      ? <MessageCircle className="h-10 w-10 text-primary/40" />
      : <Hash className="h-10 w-10 text-primary/40" />
    return (
      <div className="flex-1 flex flex-col items-center justify-center px-6">
        <EmptyState
          icon={icon}
          title={isDM ? `Chat with ${activeChannel?.name ?? 'user'}` : `Welcome to #${activeChannel?.name ?? 'channel'}`}
          description={isDM
            ? 'This is the beginning of your direct message history.'
            : 'This is the very beginning of the channel. Send a message to get things started!'}
        />
      </div>
    )
  }

  const virtualItems = virtualizer.getVirtualItems()

  return (
    <div className="flex-1 relative overflow-hidden">
      <div
        ref={parentRef}
        onScroll={handleScroll}
        className="h-full overflow-y-auto overflow-x-hidden"
      >
        {isSearching && (
          <div className="text-center py-2 text-xs text-muted-foreground flex items-center justify-center gap-1.5">
            <Search className="h-3 w-3" />
            {searching ? 'Searching...' : `${filteredMessages.length} result${filteredMessages.length !== 1 ? 's' : ''} for \u201c${searchQuery?.trim()}\u201d`}
          </div>
        )}
        {searchError && (
          <div className="px-4 py-8 text-center">
            <p className="text-sm text-destructive">{searchError}</p>
          </div>
        )}
        {!isSearching && hasMore && (
          <div className="text-center py-2 text-xs text-muted-foreground flex items-center justify-center gap-1.5">
            {loadingOlder ? (
              <><Loader2 className="h-3 w-3 animate-spin" /> Loading older messages...</>
            ) : (
              'Scroll up to load older messages'
            )}
          </div>
        )}
        <div
          style={{
            height: `${virtualizer.getTotalSize()}px`,
            width: '100%',
            position: 'relative',
          }}
        >
          {virtualItems.map((virtualRow) => {
            const msg = filteredMessages[virtualRow.index]
            const grouped = groupedFlags[virtualRow.index]
            const isSelected = !isSearching && virtualRow.index === selectedIndex

            return (
              <div
                key={virtualRow.key}
                data-index={virtualRow.index}
                ref={virtualizer.measureElement}
                style={{
                  position: 'absolute',
                  top: 0,
                  left: 0,
                  width: '100%',
                  transform: `translateY(${virtualRow.start}px)`,
                }}
              >
                {isSearching ? (
                  <button
                    onClick={() => {
                      setActiveChannel(msg.channelId)
                    }}
                    className="w-full text-left px-4 py-2 hover:bg-accent/50 border-b border-border/30 transition-colors"
                  >
                    <div className="flex items-center gap-2 text-xs text-muted-foreground mb-1">
                      <Hash className="h-3 w-3" />
                      <span className="font-medium">
                        {searchResultsView?.[virtualRow.index]?.chName || 'unknown'}
                      </span>
                      <span>·</span>
                      <span>@{msg.senderName}</span>
                      <span>·</span>
                      <span>{new Date(msg.createdAt).toLocaleString()}</span>
                    </div>
                    <div className="text-sm line-clamp-2">{msg.content}</div>
                  </button>
                ) : (
                  <div
                    className={cn('transition-colors', isSelected && 'bg-primary/5 ring-1 ring-inset ring-primary/30')}
                    data-selected={isSelected ? 'true' : undefined}
                  >
                    <MessageItem message={msg} onReply={onReply} onQuote={setQuotedMessage} onAgentClick={handleAgentClick} agentNames={agentNames} threadInfo={threadByMessageId.get(msg.id)} grouped={grouped} />
                  </div>
                )}
              </div>
            )
          })}
        </div>
      </div>
      {showNewMsg && (
        <button
          onClick={scrollToBottom}
          className="absolute bottom-4 left-1/2 -translate-x-1/2 flex items-center gap-1.5 px-3 py-1.5 rounded-full bg-primary text-primary-foreground text-xs font-medium shadow-lg hover:bg-primary/90 transition-all animate-in fade-in slide-in-from-bottom-2"
        >
          <ArrowDown className="h-3 w-3" />
          New messages
        </button>
      )}
    </div>
  )
}

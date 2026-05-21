import { useEffect, useRef, useState, useCallback, type KeyboardEvent } from 'react'
import { useThreadStore } from '@/stores/threadStore'
import { useMessageStore, EMPTY_MESSAGES } from '@/stores/messageStore'
import { toastError } from '@/stores/toastStore'
import { MessageItem } from './MessageItem'
import { MessageSkeleton } from '@/components/Skeleton'
import { X, Send } from 'lucide-react'
import { Button } from '@/components/ui'

export function ThreadPanel() {
  const threadChannel = useThreadStore((s) => s.threadChannel)
  const parentMessage = useThreadStore((s) => s.parentMessage)
  const loading = useThreadStore((s) => s.loading)
  const closeThread = useThreadStore((s) => s.closeThread)

  const messages = useMessageStore((s) => s.messagesByChannel.get(threadChannel?.id ?? '') ?? EMPTY_MESSAGES)
  const fetchMessages = useMessageStore((s) => s.fetchMessages)
  const sendMessage = useMessageStore((s) => s.sendMessage)

  const bottomRef = useRef<HTMLDivElement>(null)
  const [text, setText] = useState('')
  const [sending, setSending] = useState(false)

  // Fetch thread messages when thread opens
  useEffect(() => {
    if (threadChannel?.id) {
      fetchMessages(threadChannel.id)
    }
  }, [threadChannel?.id, fetchMessages])

  // Auto-scroll when new messages arrive
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [messages.length])

  const handleSend = useCallback(async () => {
    if (!text.trim() || !threadChannel?.id || sending) return
    setSending(true)
    try {
      await sendMessage(threadChannel.id, text.trim())
      setText('')
    } catch {
      toastError('Failed to send message')
    } finally {
      setSending(false)
    }
  }, [text, threadChannel?.id, sending, sendMessage])

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      handleSend()
    }
  }

  if (!parentMessage && !loading) return null

  return (
    <aside className="w-80 border-l flex flex-col shrink-0 bg-background">
      {/* Header */}
      <div className="h-12 border-b flex items-center justify-between px-3 shrink-0">
        <h3 className="font-semibold text-sm">Thread</h3>
        <Button
          variant="ghost"
          size="sm"
          onClick={closeThread}
        >
          <X className="h-4 w-4" />
        </Button>
      </div>

      {/* Parent message */}
      {parentMessage && (
        <div className="border-b bg-accent/30">
          <MessageItem message={parentMessage} />
        </div>
      )}

      {/* Thread messages */}
      <div className="flex-1 overflow-y-auto">
        {loading ? (
          <>
            <MessageSkeleton />
            <MessageSkeleton />
          </>
        ) : messages.length === 0 ? (
          <div className="flex items-center justify-center h-20 text-xs text-muted-foreground">
            No replies yet
          </div>
        ) : (
          messages.map((msg) => (
            <MessageItem key={msg.id} message={msg} />
          ))
        )}
        <div ref={bottomRef} />
      </div>

      {/* Reply input */}
      {threadChannel && (
        <div className="border-t p-2 shrink-0">
          <div className="flex items-end gap-2">
            <textarea
              data-message-input
              value={text}
              onChange={(e) => setText(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Reply..."
              rows={1}
              className="flex-1 resize-none rounded-lg border bg-background px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring min-h-[36px] max-h-[120px]"
              style={{ height: 'auto', overflow: 'hidden' }}
              onInput={(e) => {
                const target = e.target as HTMLTextAreaElement
                target.style.height = 'auto'
                target.style.height = Math.min(target.scrollHeight, 120) + 'px'
              }}
            />
            <Button
              variant="primary"
              onClick={handleSend}
              disabled={!text.trim() || sending}
              className="h-[36px] w-[36px] shrink-0 p-0"
            >
              <Send className="h-3.5 w-3.5" />
            </Button>
          </div>
        </div>
      )}
    </aside>
  )
}

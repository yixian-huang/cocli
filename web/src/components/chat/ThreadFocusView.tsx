import { useEffect, useRef, useState, useCallback, type KeyboardEvent } from 'react'
import { useThreadStore } from '@/stores/threadStore'
import { useMessageStore, EMPTY_MESSAGES } from '@/stores/messageStore'
import { toastError } from '@/stores/toastStore'
import { MessageItem } from './MessageItem'
import { MessageSkeleton } from '@/components/Skeleton'
import { ArrowLeft, Send, MessageSquare } from 'lucide-react'
import { Button } from '@/components/ui'

export function ThreadFocusView() {
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

  useEffect(() => {
    if (threadChannel?.id) {
      fetchMessages(threadChannel.id)
    }
  }, [threadChannel?.id, fetchMessages])

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
    <div className="flex-1 flex flex-col min-h-0">
      {/* Header with back button */}
      <div className="h-12 border-b flex items-center gap-3 px-4 shrink-0">
        <button
          onClick={closeThread}
          className="p-1.5 rounded hover:bg-accent text-muted-foreground transition-colors"
          title="Back to channel"
        >
          <ArrowLeft className="h-4 w-4" />
        </button>
        <MessageSquare className="h-4 w-4 text-primary" />
        <h3 className="font-semibold text-sm">Thread</h3>
        <span className="text-xs text-muted-foreground">
          {messages.length} {messages.length === 1 ? 'reply' : 'replies'}
        </span>
      </div>

      {/* Scrollable content: parent message + replies */}
      <div className="flex-1 overflow-y-auto overflow-x-hidden">
        {/* Parent message */}
        {parentMessage && (
          <div className="border-b bg-accent/20 px-0 py-1">
            <MessageItem message={parentMessage} />
          </div>
        )}

        {/* Thread replies */}
        {loading ? (
          <div className="p-4">
            <MessageSkeleton />
            <MessageSkeleton />
          </div>
        ) : messages.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-12 text-muted-foreground gap-2">
            <MessageSquare className="h-8 w-8 opacity-30" />
            <span className="text-sm">No replies yet. Start the conversation!</span>
          </div>
        ) : (
          <div className="py-1">
            {messages.map((msg) => (
              <MessageItem key={msg.id} message={msg} />
            ))}
          </div>
        )}
        <div ref={bottomRef} />
      </div>

      {/* Reply input */}
      {threadChannel && (
        <div className="border-t p-3 shrink-0">
          <div className="flex items-end gap-2">
            <textarea
              data-message-input
              value={text}
              onChange={(e) => setText(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Reply in thread..."
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
    </div>
  )
}

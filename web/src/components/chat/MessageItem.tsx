import { useMemo, useState, useCallback, memo } from 'react'
import type { Message, PriorityClass } from '@/lib/types'
import { cn, relativeTime } from '@/lib/utils'
import { usePresenceStore } from '@/stores/presenceStore'
import { useUserStore } from '@/stores/userStore'
import { ImageLightbox } from './ImageLightbox'
import { MarkdownRenderer } from './MarkdownRenderer'
import { CodeBlock } from './CodeBlock'
import { BlockRenderer } from './blocks/BlockRenderer'
import { pins as pinsApi } from '@/api/client'
import { MessageSquare, Pin, FileText, FileCode, FileArchive, File, Quote, Link, Bookmark } from 'lucide-react'
import { useBookmarkStore } from '@/stores/bookmarkStore'
import { MessageReactions } from './MessageReactions'
import { toast } from '@/stores/toastStore'
import { Avatar, Badge } from '@/components/ui'

interface ThreadInfo {
  replyCount: number
  lastActivityAt: string
}

interface Props {
  message: Message
  onReply?: (message: Message) => void
  onQuote?: (message: Message) => void
  onPinToggle?: () => void
  onAgentClick?: (agentId: string) => void
  agentNames?: Set<string>
  threadInfo?: ThreadInfo
  grouped?: boolean
}

function highlightMentions(content: string): string {
  return content.replace(/@(\w+)/g, '`@$1`')
}

const IMAGE_EXTS = /\.(png|jpg|jpeg|gif|webp|svg|bmp)$/i

function fileIcon(name: string) {
  if (/\.(js|ts|tsx|jsx|py|go|rs|java|c|cpp|rb|sh)$/i.test(name)) return <FileCode className="h-4 w-4" />
  if (/\.(zip|tar|gz|rar|7z)$/i.test(name)) return <FileArchive className="h-4 w-4" />
  if (/\.(pdf|doc|docx|txt|md)$/i.test(name)) return <FileText className="h-4 w-4" />
  return <File className="h-4 w-4" />
}

// R2 (2026-04-25): sender-side urgency was removed; the daemon now drives
// priority-class via rule classifier. Visual elevation badges still respect
// the resolved priority_class on the message (server-emitted), just no
// longer the user-supplied urgency field.
function resolveUrgency(message: Message): PriorityClass | null {
  const raw = message.priorityClass
  if (raw === 'critical' || raw === 'high') return raw
  return null
}

export const MessageItem = memo(function MessageItem({ message, onReply, onQuote, onPinToggle, onAgentClick, agentNames, threadInfo, grouped }: Props) {
  const isSystem = message.messageType === 'system' || message.senderType === 'system'
  const isAgent = message.senderType === 'agent'
  const isPrivateMessage = message.visibility === 'private' || message.hideFromAgents || message.hiddenFromAgents
  const urgency = resolveUrgency(message)
  const urgencyVariant = urgency === 'critical' ? 'error' : 'warning'
  const urgencyRowClass = urgency === 'critical'
    ? 'border-l-2 border-error/70 bg-error/5'
    : urgency === 'high'
      ? 'border-l-2 border-warning/70 bg-warning/5'
      : ''
  const online = usePresenceStore((s) => message.senderId ? s.onlineIds.has(message.senderId) : false)
  const currentUser = useUserStore((s) => s.user?.name)
  const isMentioned = currentUser ? message.content.includes(`@${currentUser}`) : false
  const renderedContent = useMemo(() => highlightMentions(message.content), [message.content])
  const [lightboxSrc, setLightboxSrc] = useState<string | null>(null)

  const isBookmarked = useBookmarkStore((s) => s.bookmarkedIds.has(message.id))
  const toggleBookmark = useBookmarkStore((s) => s.toggleBookmark)

  const handlePinToggle = useCallback(async () => {
    try {
      if (message.pinned) {
        await pinsApi.unpin(message.id)
      } else {
        await pinsApi.pin(message.id)
      }
      onPinToggle?.()
    } catch {
      // ignore
    }
  }, [message.id, message.pinned, onPinToggle])

  const mdComponents = useMemo(() => ({
    code: ({ className, children }: { className?: string; children?: React.ReactNode }) => {
      const text = String(children || '')
      if (!className && text.startsWith('@')) {
        const name = text.slice(1)
        const isAgentMention = agentNames?.has(name) ?? false
        return (
          <span className={cn(
            'inline-flex items-center px-1.5 py-0.5 rounded-md text-xs font-medium',
            isAgentMention
              ? 'bg-primary/15 text-primary border border-primary/20'
              : 'bg-accent/80 text-foreground'
          )}>
            {text}
          </span>
        )
      }
      return <CodeBlock className={className}>{children}</CodeBlock>
    },
    a: ({ href, children }: { href?: string; children?: React.ReactNode }) => {
      const text = String(children || '')
      if (href && IMAGE_EXTS.test(href)) {
        return (
          <img
            src={href}
            alt={text}
            className="max-w-xs max-h-48 rounded-lg mt-1 cursor-pointer hover:opacity-90 transition-opacity"
            onClick={() => setLightboxSrc(href)}
          />
        )
      }
      if (href && /^\/api\/attachments\//.test(href)) {
        return (
          <a href={href} target="_blank" rel="noopener noreferrer"
            className="inline-flex items-center gap-1.5 px-2 py-1 rounded bg-accent/60 text-xs hover:bg-accent no-underline">
            {fileIcon(text)}
            <span>{text}</span>
          </a>
        )
      }
      return <a href={href} target="_blank" rel="noopener noreferrer">{children}</a>
    },
    img: ({ src, alt }: { src?: string; alt?: string }) => (
      <img
        src={src}
        alt={alt || ''}
        className="max-w-xs max-h-48 rounded-lg mt-1 cursor-pointer hover:opacity-90 transition-opacity"
        onClick={() => src && setLightboxSrc(src)}
      />
    ),
  }), [agentNames])
  const isOnline = online

  const lightbox = lightboxSrc ? <ImageLightbox src={lightboxSrc} onClose={() => setLightboxSrc(null)} /> : null

  // System messages render as compact inline notifications
  if (isSystem) {
    return (
      <div className="flex items-center gap-2 px-4 py-1">
        <div className="h-px flex-1 bg-border/50" />
        <span className="text-xs text-muted-foreground shrink-0">
          {message.content}
        </span>
        <span className="font-signal text-[10px] uppercase tracking-[0.12em] text-content-subtle shrink-0">
          {new Date(message.createdAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
        </span>
        <div className="h-px flex-1 bg-border/50" />
      </div>
    )
  }

  if (grouped) {
    return (
      <>
        <div className={cn('flex gap-3 px-4 py-0.5 hover:bg-accent/30 group relative', isAgent && 'border-l-2 border-l-accent-signature bg-surface-panel', isMentioned && !isAgent && 'bg-primary/5 border-l-2 border-primary/40', urgencyRowClass)}>
          <div className="w-8 shrink-0 flex items-center justify-center">
            <span className="font-signal text-[10px] uppercase tracking-[0.12em] text-content-subtle opacity-0 group-hover:opacity-100 transition-opacity">
              {new Date(message.createdAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
            </span>
          </div>
          <div className="min-w-0 flex-1">
            {urgency && (
              <div className="mb-1">
                <Badge size="sm" variant={urgencyVariant}>
                  {urgency}
                </Badge>
              </div>
            )}
            {message.blocks && message.blocks.length > 0 ? (
              <BlockRenderer blocks={message.blocks} messageId={message.id} />
            ) : (
              <div className="prose prose-sm dark:prose-invert max-w-full leading-snug overflow-hidden [&>*:first-child]:mt-0 [&>*:last-child]:mb-0">
                <MarkdownRenderer components={mdComponents}>{renderedContent}</MarkdownRenderer>
              </div>
            )}
            {isPrivateMessage && (
              <div className="mt-1">
                <Badge size="sm" variant="warning">private</Badge>
              </div>
            )}
            <MessageReactions messageId={message.id} />
          </div>
          <div className="absolute right-2 top-1 opacity-0 group-hover:opacity-100 flex items-center gap-0.5 transition-opacity">
            <button
              onClick={() => {
                const url = `${window.location.origin}/channel/${message.channelId}/msg/${message.id}`
                navigator.clipboard.writeText(url)
                toast('Link copied', 'success')
              }}
              className="p-1 rounded hover:bg-accent text-muted-foreground"
              title="Copy link"
            >
              <Link className="h-3.5 w-3.5" />
            </button>
            <button
              onClick={handlePinToggle}
              className={cn('p-1 rounded hover:bg-accent text-muted-foreground', message.pinned && 'text-primary')}
              title={message.pinned ? 'Unpin' : 'Pin'}
            >
              <Pin className="h-3.5 w-3.5" />
            </button>
            <button
              onClick={() => toggleBookmark(message.id)}
              className="p-1 rounded hover:bg-accent"
              title={isBookmarked ? 'Remove bookmark' : 'Save message'}
            >
              <Bookmark className={`h-3.5 w-3.5 ${isBookmarked ? 'fill-current text-primary' : 'text-muted-foreground'}`} />
            </button>
            {onQuote && (
              <button
                onClick={() => onQuote(message)}
                className="p-1 rounded hover:bg-accent text-muted-foreground"
                title="Quote"
              >
                <Quote className="h-3.5 w-3.5" />
              </button>
            )}
            {onReply && (
              threadInfo && threadInfo.replyCount > 0 ? (
                <button
                  onClick={() => onReply(message)}
                  className="flex items-center gap-1 px-1.5 py-0.5 rounded bg-primary/10 text-primary hover:bg-primary/20 text-[11px] font-medium transition-colors"
                  title="Reply in thread"
                >
                  <MessageSquare className="h-3 w-3" />
                  Reply in thread
                </button>
              ) : (
                <button
                  onClick={() => onReply(message)}
                  className="p-1 rounded hover:bg-accent text-muted-foreground"
                  title="Reply in thread"
                >
                  <MessageSquare className="h-3.5 w-3.5" />
                </button>
              )
            )}
          </div>
        </div>
        {lightbox}
      </>
    )
  }

  return (
    <>
      <div className={cn('flex gap-3 px-4 pt-2 pb-1 hover:bg-accent/30 group relative border-b border-border/30', isAgent && 'border-l-2 border-l-accent-signature bg-surface-panel', isMentioned && !isAgent && 'bg-primary/5 border-l-2 border-primary/40', urgencyRowClass)}>
        <div className="relative shrink-0">
          {isAgent && message.senderId && onAgentClick ? (
            <button
              onClick={() => onAgentClick(message.senderId!)}
              className="cursor-pointer hover:opacity-80 transition-opacity"
              title={`View @${message.senderName}`}
            >
              <Avatar
                name={message.senderName}
                isBot={isAgent}
                status={isOnline ? 'online' : undefined}
              />
            </button>
          ) : (
            <Avatar
              name={message.senderName}
              isBot={isAgent}
              status={isOnline ? 'online' : undefined}
            />
          )}
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-baseline gap-2">
            <span
              className={cn(
                'font-semibold text-sm',
                isAgent ? 'text-accent-signature' : 'text-content-primary'
              )}
            >
              {message.senderName}
            </span>
            {isAgent && (
              <Badge variant="info" size="sm">BOT</Badge>
            )}
            {urgency && (
              <Badge variant={urgencyVariant} size="sm">{urgency}</Badge>
            )}
            <span
              className="font-signal text-[10px] uppercase tracking-[0.12em] text-content-subtle"
              title={new Date(message.createdAt).toLocaleString()}
            >
              {relativeTime(message.createdAt)}
            </span>
            {message.pinned && <Pin className="h-3 w-3 text-primary" />}
            {isPrivateMessage && <Badge size="sm" variant="warning">private</Badge>}
          </div>
          {message.blocks && message.blocks.length > 0 ? (
            <BlockRenderer blocks={message.blocks} messageId={message.id} />
          ) : (
            <div className="prose prose-sm dark:prose-invert max-w-full leading-snug overflow-hidden mt-0.5 [&>*:first-child]:mt-0 [&>*:last-child]:mb-0">
              <MarkdownRenderer components={mdComponents}>{renderedContent}</MarkdownRenderer>
            </div>
          )}
          <MessageReactions messageId={message.id} />
          {threadInfo && threadInfo.replyCount > 0 && onReply && (
            <button
              onClick={() => onReply(message)}
              className="flex items-center gap-1.5 mt-1 text-xs text-primary hover:text-primary/80 transition-colors"
            >
              <MessageSquare className="h-3 w-3" />
              <span className="font-medium">{threadInfo.replyCount} {threadInfo.replyCount === 1 ? 'reply' : 'replies'}</span>
              <span className="text-muted-foreground">&middot; last {relativeTime(threadInfo.lastActivityAt)}</span>
            </button>
          )}
        </div>
        <div className="absolute right-2 top-2 opacity-0 group-hover:opacity-100 flex items-center gap-0.5 transition-opacity">
          <button
            onClick={() => {
              const url = `${window.location.origin}/channel/${message.channelId}/msg/${message.id}`
              navigator.clipboard.writeText(url)
              toast('Link copied', 'success')
            }}
            className="p-1 rounded hover:bg-accent text-muted-foreground"
            title="Copy link"
          >
            <Link className="h-3.5 w-3.5" />
          </button>
          <button
            onClick={handlePinToggle}
            className={cn('p-1 rounded hover:bg-accent text-muted-foreground', message.pinned && 'text-primary')}
            title={message.pinned ? 'Unpin' : 'Pin'}
          >
            <Pin className="h-3.5 w-3.5" />
          </button>
          <button
            onClick={() => toggleBookmark(message.id)}
            className="p-1 rounded hover:bg-accent"
            title={isBookmarked ? 'Remove bookmark' : 'Save message'}
          >
            <Bookmark className={`h-3.5 w-3.5 ${isBookmarked ? 'fill-current text-primary' : 'text-muted-foreground'}`} />
          </button>
          {onReply && (
            threadInfo && threadInfo.replyCount > 0 ? (
              <button
                onClick={() => onReply(message)}
                className="flex items-center gap-1 px-1.5 py-0.5 rounded bg-primary/10 text-primary hover:bg-primary/20 text-[11px] font-medium transition-colors"
                title="Reply in thread"
              >
                <MessageSquare className="h-3 w-3" />
                Reply in thread
              </button>
            ) : (
              <button
                onClick={() => onReply(message)}
                className="p-1 rounded hover:bg-accent text-muted-foreground"
                title="Reply in thread"
              >
                <MessageSquare className="h-3.5 w-3.5" />
              </button>
            )
          )}
        </div>
      </div>
      {lightbox}
    </>
  )
}, (prev, next) =>
  prev.message.id === next.message.id &&
  prev.message.content === next.message.content &&
  prev.message.pinned === next.message.pinned &&
  prev.message.blocks === next.message.blocks &&
  prev.message.priorityClass === next.message.priorityClass &&
  prev.grouped === next.grouped &&
  prev.agentNames === next.agentNames &&
  prev.threadInfo === next.threadInfo
)

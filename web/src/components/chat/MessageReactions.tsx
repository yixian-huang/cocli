import { useState, useEffect, useCallback } from 'react'
import type { ReactionSummary } from '@/api/client'
import { useUserStore } from '@/stores/userStore'
import { useReactionStore } from '@/stores/reactionStore'
import { cn } from '@/lib/utils'
import { SmilePlus } from 'lucide-react'

const QUICK_EMOJIS = ['👍', '❤️', '😂', '🎉', '👀', '🚀', '✅', '❌']
const EMPTY_SUMMARIES: ReactionSummary[] = []

interface Props {
  messageId: string
}

export function MessageReactions({ messageId }: Props) {
  const [showPicker, setShowPicker] = useState(false)
  const currentUserId = useUserStore((s) => s.user?.id)
  const summaries = useReactionStore((s) => s.summariesByMessage.get(messageId) ?? EMPTY_SUMMARIES)
  const loaded = useReactionStore((s) => s.loaded.has(messageId))
  const ensureLoaded = useReactionStore((s) => s.ensureLoaded)
  const toggleReaction = useReactionStore((s) => s.toggleReaction)

  useEffect(() => {
    if (!loaded) ensureLoaded(messageId)
  }, [loaded, messageId, ensureLoaded])

  const handleToggle = useCallback(async (emoji: string) => {
    if (!currentUserId) return
    await toggleReaction(messageId, emoji, currentUserId)
    setShowPicker(false)
  }, [messageId, currentUserId, toggleReaction])

  return (
    <div className="flex flex-wrap items-center gap-1 mt-1 relative">
      {summaries.map((s) => {
        const hasReacted = currentUserId ? s.userIds.includes(currentUserId) : false
        return (
          <button
            key={s.emoji}
            onClick={() => handleToggle(s.emoji)}
            className={cn(
              'inline-flex items-center gap-1 px-1.5 py-0.5 rounded-full text-xs border transition-colors',
              hasReacted
                ? 'bg-primary/10 border-primary/30 text-primary'
                : 'bg-muted/50 border-border hover:bg-accent',
            )}
          >
            <span>{s.emoji}</span>
            <span className="font-medium">{s.count}</span>
          </button>
        )
      })}
      <div className="relative">
        <button
          onClick={() => setShowPicker(!showPicker)}
          className="p-1 rounded-full hover:bg-accent text-muted-foreground transition-colors opacity-0 group-hover:opacity-100"
          title="Add reaction"
        >
          <SmilePlus className="h-3.5 w-3.5" />
        </button>
        {showPicker && (
          <div className="absolute bottom-full left-0 mb-1 p-1.5 rounded-lg border bg-background shadow-lg flex gap-1 z-10">
            {QUICK_EMOJIS.map((emoji) => (
              <button
                key={emoji}
                onClick={() => handleToggle(emoji)}
                className="p-1 rounded hover:bg-accent text-sm transition-colors"
              >
                {emoji}
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}

import { useEffect, useMemo, useState, type KeyboardEvent } from 'react'
import { useNavigate } from 'react-router-dom'
import { Hash, MessageCircle, Search } from 'lucide-react'
import { useChannelStore } from '@/stores/channelStore'
import { useWorkspacePanelStore } from '@/stores/workspacePanelStore'
import { cn } from '@/lib/utils'
import type { Channel } from '@/lib/types'
import { channelPath } from '@/lib/paths'
import { Input } from './Input'
import { Modal } from './Modal'

interface ChannelSwitcherProps {
  open: boolean
  onClose: () => void
}

interface RankedChannel {
  channel: Channel
  score: number
}

function fuzzyScore(channel: Channel, query: string) {
  if (!query) return 0

  const text = `${channel.displayName || ''} ${channel.name}`.trim().toLowerCase()
  if (text === query) return 1000
  if (text.startsWith(query)) return 800

  const containsIndex = text.indexOf(query)
  if (containsIndex >= 0) return 600 - containsIndex

  let queryIndex = 0
  let score = 0
  for (let i = 0; i < text.length && queryIndex < query.length; i += 1) {
    if (text[i] === query[queryIndex]) {
      score += 10
      queryIndex += 1
    }
  }

  return queryIndex === query.length ? score : -1
}

function rankChannels(channels: Channel[], query: string) {
  const normalized = query.trim().toLowerCase()
  const ranked: RankedChannel[] = channels
    .map((channel) => ({ channel, score: fuzzyScore(channel, normalized) }))
    .filter((entry) => normalized.length === 0 || entry.score >= 0)

  ranked.sort((left, right) => {
    if (left.score !== right.score) return right.score - left.score
    return (left.channel.displayName || left.channel.name).localeCompare(right.channel.displayName || right.channel.name)
  })

  return ranked.map((entry) => entry.channel)
}

export function ChannelSwitcher({ open, onClose }: ChannelSwitcherProps) {
  const navigate = useNavigate()
  const channels = useChannelStore((state) => state.channels)
  const dmChannels = useChannelStore((state) => state.dmChannels)
  const activeChannelId = useChannelStore((state) => state.activeChannelId)
  const setActiveChannel = useChannelStore((state) => state.setActiveChannel)
  const setPanel = useWorkspacePanelStore((state) => state.setPanel)

  const [query, setQuery] = useState('')
  const [selectedIndex, setSelectedIndex] = useState(0)

  const results = useMemo(() => rankChannels([...channels, ...dmChannels], query), [channels, dmChannels, query])

  useEffect(() => {
    if (!open) return
    setQuery('')
    setSelectedIndex(0)
  }, [open])

  useEffect(() => {
    if (results.length === 0) {
      setSelectedIndex(0)
      return
    }
    setSelectedIndex((current) => Math.min(current, results.length - 1))
  }, [results])

  const handleSelect = (channel: Channel) => {
    setPanel('chat')
    setActiveChannel(channel.id)
    navigate(channelPath({ channelId: channel.id }))
    onClose()
  }

  const handleKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'ArrowDown') {
      event.preventDefault()
      setSelectedIndex((current) => Math.min(current + 1, Math.max(results.length - 1, 0)))
      return
    }
    if (event.key === 'ArrowUp') {
      event.preventDefault()
      setSelectedIndex((current) => Math.max(current - 1, 0))
      return
    }
    if (event.key === 'Home') {
      event.preventDefault()
      setSelectedIndex(0)
      return
    }
    if (event.key === 'End') {
      event.preventDefault()
      setSelectedIndex(Math.max(results.length - 1, 0))
      return
    }
    if (event.key === 'Enter') {
      const channel = results[selectedIndex]
      if (!channel) return
      event.preventDefault()
      handleSelect(channel)
    }
  }

  return (
    <Modal open={open} onClose={onClose} title="Switch channel" size="md">
      <div className="space-y-3" data-testid="channel-switcher">
        <Input
          autoFocus
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Jump to a channel or DM..."
          iconLeft={<Search className="h-3.5 w-3.5" />}
          aria-label="Channel switcher search"
        />

        <div className="max-h-72 overflow-y-auto rounded-[var(--radius-base)] bg-surface-panel">
          {results.length === 0 ? (
            <div className="px-3 py-8 text-center text-sm text-content-muted">
              No matching channels.
            </div>
          ) : (
            results.map((channel, index) => {
              const isSelected = index === selectedIndex
              const isActive = channel.id === activeChannelId
              const Icon = channel.type === 'dm' ? MessageCircle : Hash
              return (
                <button
                  key={channel.id}
                  type="button"
                  onClick={() => handleSelect(channel)}
                  onMouseEnter={() => setSelectedIndex(index)}
                  className={cn(
                    'flex w-full items-center gap-3 px-4 py-2 text-left text-sm transition-colors',
                    'border-l-2 border-transparent',
                    isSelected
                      ? 'bg-surface-raised border-l-accent-signature text-content-primary'
                      : 'hover:bg-surface-raised text-content-secondary',
                  )}
                  aria-selected={isSelected}
                >
                  <Icon className={cn('h-4 w-4 shrink-0', isSelected ? 'text-accent-signature' : 'text-content-subtle')} />
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm font-medium">
                      {channel.displayName || channel.name}
                    </div>
                    <div className="truncate text-xs text-content-muted">
                      {channel.type === 'dm' ? 'Direct message' : `#${channel.name}`}
                    </div>
                  </div>
                  {isActive ? (
                    <span className="ml-auto font-signal text-[10px] uppercase tracking-[0.08em] text-content-subtle bg-surface-raised px-1.5 py-0.5 rounded-sm">
                      Current
                    </span>
                  ) : null}
                </button>
              )
            })
          )}
        </div>
      </div>
    </Modal>
  )
}

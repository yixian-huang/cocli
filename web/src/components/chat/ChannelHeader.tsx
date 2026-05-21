import { useChannelStore } from '@/stores/channelStore'
import { Hash, MessageCircle, Search, Settings, X } from 'lucide-react'
import { Button } from '@/components/ui'

interface Props {
  searchOpen: boolean
  searchQuery: string
  onSearchToggle: () => void
  onSearchChange: (query: string) => void
  onSettingsToggle?: () => void
  settingsOpen?: boolean
}

export function ChannelHeader({ searchOpen, searchQuery, onSearchToggle, onSearchChange, onSettingsToggle, settingsOpen }: Props) {
  const activeId = useChannelStore((s) => s.activeChannelId)
  const channels = useChannelStore((s) => s.channels)
  const dms = useChannelStore((s) => s.dmChannels)

  const channel = [...channels, ...dms].find((c) => c.id === activeId)
  if (!channel) return null

  const Icon = channel.type === 'dm' ? MessageCircle : Hash

  return (
    <div className="h-12 border-b flex items-center px-4 shrink-0 gap-2">
      <Icon className="h-4 w-4 shrink-0 text-muted-foreground" />
      <h2 className="font-semibold text-sm">{channel.displayName || channel.name}</h2>
      {channel.description && !searchOpen && (
        <span className="ml-1 text-xs text-muted-foreground truncate hidden sm:inline">
          {channel.description}
        </span>
      )}
      <div className="ml-auto flex items-center gap-2">
        {searchOpen ? (
          <div className="flex items-center gap-1">
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => onSearchChange(e.target.value)}
              placeholder="Search messages..."
              autoFocus
              className="w-40 sm:w-56 rounded border bg-background px-2 py-1 text-xs focus:outline-none focus:ring-1 focus:ring-ring"
            />
            <Button
              variant="ghost"
              size="sm"
              onClick={onSearchToggle}
            >
              <X className="h-3.5 w-3.5" />
            </Button>
          </div>
        ) : (
          <>
            {channel.memberCount != null && (
              <span className="text-xs text-muted-foreground hidden sm:inline">
                {channel.memberCount} members
              </span>
            )}
            <Button
              variant="ghost"
              size="sm"
              onClick={onSearchToggle}
              title="Search messages"
            >
              <Search className="h-3.5 w-3.5" />
            </Button>
            {channel.type === 'channel' && onSettingsToggle && (
              <Button
                variant="ghost"
                size="sm"
                onClick={onSettingsToggle}
                className={settingsOpen ? 'bg-accent text-foreground' : ''}
                title="Channel settings"
              >
                <Settings className="h-3.5 w-3.5" />
              </Button>
            )}
          </>
        )}
      </div>
    </div>
  )
}

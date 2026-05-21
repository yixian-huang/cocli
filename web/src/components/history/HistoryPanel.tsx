import { useEffect, useMemo } from 'react'
import { useNavigate } from 'react-router-dom'
import { useZoneStore } from '@/stores/zoneStore'
import { useHistoryStore } from '@/stores/historyStore'
import { useChannelStore } from '@/stores/channelStore'
import { useAgentStore } from '@/stores/agentStore'
import { useUserStore } from '@/stores/userStore'
import { useWorkspacePanelStore } from '@/stores/workspacePanelStore'
import { Button, Badge } from '@/components/ui'
import { CalendarRange, Search, ExternalLink } from 'lucide-react'
import { messagePath } from '@/lib/paths'

function formatDate(date: string): string {
  const parsed = new Date(date)
  if (Number.isNaN(parsed.getTime())) return date
  return parsed.toLocaleString()
}

export function HistoryPanel() {
  const navigate = useNavigate()
  const activeZoneId = useZoneStore((s) => s.activeZoneId)
  const activeZoneSlug = useZoneStore((s) => s.activeZoneSlug)
  const channels = useChannelStore((s) => s.channels)
  const dms = useChannelStore((s) => s.dmChannels)
  const agents = useAgentStore((s) => s.agents)
  const users = useUserStore((s) => s.allUsers)
  const setWorkspacePanel = useWorkspacePanelStore((s) => s.setPanel)

  const items = useHistoryStore((s) => s.items)
  const page = useHistoryStore((s) => s.page)
  const pageSize = useHistoryStore((s) => s.pageSize)
  const total = useHistoryStore((s) => s.total)
  const loading = useHistoryStore((s) => s.loading)
  const error = useHistoryStore((s) => s.error)
  const filters = useHistoryStore((s) => s.filters)
  const setFilters = useHistoryStore((s) => s.setFilters)
  const setPage = useHistoryStore((s) => s.setPage)
  const fetch = useHistoryStore((s) => s.fetch)

  useEffect(() => {
    if (!activeZoneId) return
    fetch(activeZoneId)
  }, [activeZoneId, fetch, page, pageSize, filters])

  const totalPages = Math.max(1, Math.ceil(total / pageSize))
  const canPrev = page > 1
  const canNext = page < totalPages

  const senderOptions = useMemo(
    () =>
      [
        ...users.map((u) => ({ id: u.id, label: u.displayName || u.name, type: 'user' as const })),
        ...agents.map((a) => ({ id: a.id, label: a.displayName || a.name, type: 'agent' as const })),
      ].sort((a, b) => a.label.localeCompare(b.label)),
    [users, agents],
  )

  return (
    <div className="flex-1 min-h-0 flex flex-col">
      <div className="h-12 border-b px-4 flex items-center justify-between">
        <div className="flex items-center gap-2 text-sm font-semibold">
          <Search className="h-4 w-4 text-muted-foreground" />
          History
        </div>
        <Badge size="sm" variant="default">{total} results</Badge>
      </div>

      <div className="border-b px-4 py-3 grid grid-cols-1 md:grid-cols-6 gap-2 text-xs">
        <select
          className="border rounded px-2 py-1 bg-background"
          value={filters.channelId || ''}
          onChange={(e) => setFilters({ channelId: e.target.value || undefined })}
        >
          <option value="">All channels</option>
          {[...channels, ...dms].map((c) => (
            <option key={c.id} value={c.id}>
              {c.type === 'dm' ? 'DM' : '#'} {c.displayName || c.name}
            </option>
          ))}
        </select>
        <input
          className="border rounded px-2 py-1 bg-background md:col-span-2"
          placeholder="keyword"
          value={filters.q || ''}
          onChange={(e) => setFilters({ q: e.target.value || undefined })}
        />
        <select
          className="border rounded px-2 py-1 bg-background"
          value={filters.senderType || ''}
          onChange={(e) =>
            setFilters({
              senderType: (e.target.value || undefined) as typeof filters.senderType,
              senderId: undefined,
            })
          }
        >
          <option value="">All senders</option>
          <option value="user">Users</option>
          <option value="agent">Agents</option>
          <option value="system">System</option>
        </select>
        <select
          className="border rounded px-2 py-1 bg-background"
          value={filters.senderId || ''}
          onChange={(e) => setFilters({ senderId: e.target.value || undefined })}
          disabled={!filters.senderType || filters.senderType === 'system'}
        >
          <option value="">Any sender</option>
          {senderOptions
            .filter((sender) => !filters.senderType || sender.type === filters.senderType)
            .map((sender) => (
              <option key={sender.id} value={sender.id}>
                {sender.type === 'agent' ? '@' : ''}
                {sender.label}
              </option>
            ))}
        </select>
        <div className="flex items-center gap-2 md:col-span-2">
          <CalendarRange className="h-3.5 w-3.5 text-muted-foreground shrink-0" />
          <input
            className="border rounded px-2 py-1 bg-background flex-1"
            type="datetime-local"
            value={filters.from || ''}
            onChange={(e) => setFilters({ from: e.target.value || undefined })}
          />
          <span className="text-muted-foreground">→</span>
          <input
            className="border rounded px-2 py-1 bg-background flex-1"
            type="datetime-local"
            value={filters.to || ''}
            onChange={(e) => setFilters({ to: e.target.value || undefined })}
          />
        </div>
      </div>

      <div className="flex-1 min-h-0 overflow-y-auto divide-y">
        {loading ? (
          <div className="p-4 text-sm text-muted-foreground">Loading history...</div>
        ) : error ? (
          <div className="p-4 text-sm text-error">{error}</div>
        ) : items.length === 0 ? (
          <div className="p-4 text-sm text-muted-foreground">No history results</div>
        ) : (
          items.map((item) => (
            <div key={item.id} className="px-4 py-3 flex items-start gap-3 hover:bg-accent/30">
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2 text-xs text-muted-foreground mb-1">
                  <span>{item.senderType === 'agent' ? '@' : ''}{item.senderDisplayName || item.senderName}</span>
                  <span>·</span>
                  <span>{item.channelName || item.channelId}</span>
                  <span>·</span>
                  <span>{formatDate(item.createdAt)}</span>
                  <span>·</span>
                  <span>#{item.seq}</span>
                  {item.hiddenFromAgents && (
                    <>
                      <span>·</span>
                      <Badge size="sm" variant="warning">private</Badge>
                    </>
                  )}
                </div>
                <p className="text-sm whitespace-pre-wrap break-words">{item.content}</p>
              </div>
              <Button
                variant="ghost"
                size="sm"
                className="shrink-0 gap-1"
                onClick={() => {
                  setWorkspacePanel('chat')
                  navigate(messagePath({ zoneSlug: activeZoneSlug, channelId: item.channelId, messageId: item.id }))
                  window.dispatchEvent(new CustomEvent('scroll-to-message', { detail: { msgId: item.id } }))
                }}
              >
                <ExternalLink className="h-3.5 w-3.5" />
                Open
              </Button>
            </div>
          ))
        )}
      </div>

      <div className="h-12 border-t px-4 flex items-center justify-between text-xs text-muted-foreground">
        <span>
          Page {page} / {totalPages}
        </span>
        <div className="flex items-center gap-2">
          <Button size="sm" variant="secondary" disabled={!canPrev} onClick={() => setPage(page - 1)}>
            Prev
          </Button>
          <Button size="sm" variant="secondary" disabled={!canNext} onClick={() => setPage(page + 1)}>
            Next
          </Button>
        </div>
      </div>
    </div>
  )
}

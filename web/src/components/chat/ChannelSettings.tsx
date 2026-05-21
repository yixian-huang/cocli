import { useState, useEffect } from 'react'
import { useChannelStore } from '@/stores/channelStore'
import { useThreadStore } from '@/stores/threadStore'
import { channels as channelsApi } from '@/api/client'
import { cn } from '@/lib/utils'
import type { Message } from '@/lib/types'
import { X, BarChart3, Settings, Users, Shield, Book } from 'lucide-react'
import { ChannelOverviewPanel } from './ChannelOverviewPanel'
import { ChannelSettingsForm } from './ChannelSettingsForm'
import { ChannelMembersPanel } from './ChannelMembersPanel'
import { ChannelResponderPolicyPanel } from './ChannelResponderPolicyPanel'
import { ChannelMemoryPanel } from './ChannelMemoryPanel'

interface Member {
  id: string
  memberId: string
  memberType: string
}

interface Props {
  onClose: () => void
}

export function ChannelSettings({ onClose }: Props) {
  const activeId = useChannelStore((s) => s.activeChannelId)
  const channels = useChannelStore((s) => s.channels)

  const channel = channels.find((c) => c.id === activeId)

  const [members, setMembers] = useState<Member[]>([])
  const [tab, setTab] = useState<'overview' | 'settings' | 'members' | 'routing' | 'memory'>('overview')

  useEffect(() => {
    if (activeId) {
      channelsApi.getMembers(activeId).then(setMembers).catch((err) => console.warn('[api] members fetch failed:', err))
    }
  }, [activeId])

  if (!channel) return null

  return (
    <div className="w-full sm:w-80 border-l flex flex-col shrink-0 bg-background h-full">
      <div className="h-12 border-b flex items-center justify-between px-4 shrink-0">
        <h3 className="font-semibold text-sm">Channel Settings</h3>
        <button onClick={onClose} className="p-1 rounded hover:bg-accent text-muted-foreground">
          <X className="h-4 w-4" />
        </button>
      </div>

      <div className="flex border-b">
        <button
          onClick={() => setTab('overview')}
          className={cn(
            'flex-1 flex items-center justify-center gap-1.5 px-3 py-2 text-xs font-medium transition-colors',
            tab === 'overview' ? 'border-b-2 border-primary text-primary' : 'text-muted-foreground hover:text-foreground',
          )}
        >
          <BarChart3 className="h-3.5 w-3.5" />
          Overview
        </button>
        <button
          onClick={() => setTab('settings')}
          className={cn(
            'flex-1 flex items-center justify-center gap-1.5 px-3 py-2 text-xs font-medium transition-colors',
            tab === 'settings' ? 'border-b-2 border-primary text-primary' : 'text-muted-foreground hover:text-foreground',
          )}
        >
          <Settings className="h-3.5 w-3.5" />
          Settings
        </button>
        <button
          onClick={() => setTab('members')}
          className={cn(
            'flex-1 flex items-center justify-center gap-1.5 px-3 py-2 text-xs font-medium transition-colors',
            tab === 'members' ? 'border-b-2 border-primary text-primary' : 'text-muted-foreground hover:text-foreground',
          )}
        >
          <Users className="h-3.5 w-3.5" />
          Members ({members.length})
        </button>
        <button
          onClick={() => setTab('routing')}
          className={cn(
            'flex-1 flex items-center justify-center gap-1.5 px-3 py-2 text-xs font-medium transition-colors',
            tab === 'routing' ? 'border-b-2 border-primary text-primary' : 'text-muted-foreground hover:text-foreground',
          )}
        >
          <Shield className="h-3.5 w-3.5" />
          Routing
        </button>
        <button
          onClick={() => setTab('memory')}
          className={cn(
            'flex-1 flex items-center justify-center gap-1.5 px-3 py-2 text-xs font-medium transition-colors',
            tab === 'memory' ? 'border-b-2 border-primary text-primary' : 'text-muted-foreground hover:text-foreground',
          )}
        >
          <Book className="h-3.5 w-3.5" />
          Memory
        </button>
      </div>

      <div className="flex-1 overflow-y-auto p-4">
        {tab === 'overview' && (
          <ChannelOverviewPanel
            channelId={activeId!}
            channelName={channel.name}
            members={members}
            onOpenThread={(_threadChannel, parentMessage: Message) => {
              useThreadStore.getState().openThread(activeId!, parentMessage)
            }}
          />
        )}

        {tab === 'settings' && (
          <ChannelSettingsForm
            channel={channel}
            channelId={activeId!}
          />
        )}

        {tab === 'members' && (
          <ChannelMembersPanel
            channelId={activeId!}
            members={members}
            onMembersChange={setMembers}
          />
        )}

        {tab === 'routing' && (
          <ChannelResponderPolicyPanel
            channelId={activeId!}
            members={members}
          />
        )}

        {tab === 'memory' && (
          <ChannelMemoryPanel channelId={activeId!} />
        )}
      </div>
    </div>
  )
}

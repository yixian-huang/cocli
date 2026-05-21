import { useAgentStore } from '@/stores/agentStore'
import { channels as channelsApi } from '@/api/client'
import { toast, toastError } from '@/stores/toastStore'
import { Bot, User, UserPlus, UserMinus } from 'lucide-react'
import { Button, Badge } from '@/components/ui'

interface Member {
  id: string
  memberId: string
  memberType: string
}

interface Props {
  channelId: string
  members: Member[]
  onMembersChange: (members: Member[]) => void
}

export function ChannelMembersPanel({ channelId, members, onMembersChange }: Props) {
  const allUsers: { id: string; name: string; role?: string }[] = []
  const agents = useAgentStore((s) => s.agents)

  const handleAddMember = async (memberId: string, memberType: string) => {
    try {
      await channelsApi.addMember(channelId, memberId, memberType)
      const updated = await channelsApi.getMembers(channelId)
      onMembersChange(updated)
      toast('Member added', 'success')
    } catch (err) {
      toastError(err instanceof Error ? err.message : 'Failed to add member')
    }
  }

  const handleRemoveMember = async (memberId: string, memberType: string) => {
    try {
      await channelsApi.removeMember(channelId, memberId, memberType)
      onMembersChange(members.filter((m) => !(m.memberId === memberId && m.memberType === memberType)))
      toast('Member removed', 'success')
    } catch (err) {
      toastError(err instanceof Error ? err.message : 'Failed to remove member')
    }
  }

  const memberIds = new Set(members.map((m) => m.memberId))
  const nonMembers = [
    ...allUsers.filter((u) => !memberIds.has(u.id)).map((u) => ({ id: u.id, name: u.name, type: 'user' as const })),
    ...agents.filter((a) => !memberIds.has(a.id)).map((a) => ({ id: a.id, name: a.name, type: 'agent' as const })),
  ]

  const memberNames = members.map((m) => {
    const user = allUsers.find((u) => u.id === m.memberId)
    const agent = agents.find((a) => a.id === m.memberId)
    return { ...m, name: user?.name || agent?.name || m.memberId, isAgent: m.memberType === 'agent', isAdmin: user?.role === 'admin' }
  })

  return (
    <div className="space-y-3">
      <div className="space-y-1">
        {memberNames.map((m) => (
          <div key={m.id} className="flex items-center gap-2 px-2 py-1.5 rounded-lg hover:bg-accent/50">
            {m.isAgent ? (
              <Bot className="h-4 w-4 text-primary shrink-0" />
            ) : (
              <User className="h-4 w-4 text-muted-foreground shrink-0" />
            )}
            <span className="text-sm flex-1 truncate">
              {m.isAgent ? '@' : ''}{m.name}
              {m.isAdmin && (
                <Badge size="sm" className="ml-1.5">Admin</Badge>
              )}
            </span>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => handleRemoveMember(m.memberId, m.memberType)}
              title="Remove member"
            >
              <UserMinus className="h-3.5 w-3.5" />
            </Button>
          </div>
        ))}
      </div>

      {nonMembers.length > 0 && (
        <>
          <div className="border-t pt-3">
            <h4 className="text-xs font-medium text-muted-foreground mb-2">Add Members</h4>
            <div className="space-y-1">
              {nonMembers.map((nm) => (
                <div key={nm.id} className="flex items-center gap-2 px-2 py-1.5 rounded-lg hover:bg-accent/50">
                  {nm.type === 'agent' ? (
                    <Bot className="h-4 w-4 text-primary shrink-0" />
                  ) : (
                    <User className="h-4 w-4 text-muted-foreground shrink-0" />
                  )}
                  <span className="text-sm flex-1 truncate">
                    {nm.type === 'agent' ? '@' : ''}{nm.name}
                  </span>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => handleAddMember(nm.id, nm.type)}
                    title="Add member"
                  >
                    <UserPlus className="h-3.5 w-3.5" />
                  </Button>
                </div>
              ))}
            </div>
          </div>
        </>
      )}
    </div>
  )
}

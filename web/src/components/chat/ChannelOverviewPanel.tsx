import { useState, useEffect } from 'react'
import { useAgentStore } from '@/stores/agentStore'
import { useTaskStore } from '@/stores/taskStore'
import { threads as threadsApi, agentWorkspace } from '@/api/client'
import { cn } from '@/lib/utils'
import type { Channel, Message, Task } from '@/lib/types'
import { Bot, ListTodo, MessageSquare, FileText, Loader2 } from 'lucide-react'
import { StatusDot, Badge, SectionHeader } from '@/components/ui'
import { agentStatusLabel, taskStatusVariant, taskStatusLabel } from '@/lib/status'

const EMPTY_TASKS: Task[] = []

interface Member {
  id: string
  memberId: string
  memberType: string
}

interface Props {
  channelId: string
  channelName: string
  members: Member[]
  onOpenThread: (threadChannel: Channel, parentMessage: Message) => void
}

export function ChannelOverviewPanel({ channelId, channelName, members, onOpenThread }: Props) {
  const agents = useAgentStore((s) => s.agents)
  const tasks = useTaskStore((s) => s.tasksByChannel.get(channelId) ?? EMPTY_TASKS)
  const fetchTasks = useTaskStore((s) => s.fetchTasks)
  const [threads, setThreads] = useState<Channel[]>([])
  const [contextContent, setContextContent] = useState<string | null>(null)
  const [contextLoading, setContextLoading] = useState(false)
  const [contextAgent, setContextAgent] = useState<string | null>(null)

  useEffect(() => {
    fetchTasks(channelId)
    threadsApi.list(channelId).then(setThreads).catch((err) => console.warn('[api] overview data fetch failed:', err))
  }, [channelId, fetchTasks])

  // Load context.md from first online agent's workspace
  useEffect(() => {
    const agentIds = members.filter((m) => m.memberType === 'agent').map((m) => m.memberId)
    const onlineAgent = agentIds.map((id) => agents.find((a) => a.id === id)).find((a) => a && a.status !== 'offline')
    if (!onlineAgent || !channelName) return
    setContextLoading(true)
    agentWorkspace.readFile(onlineAgent.id, `channels/${channelName}/context.md`).then((res) => {
      if (res.content) {
        setContextContent(res.content)
        setContextAgent(onlineAgent.name)
      }
    }).catch(() => {
      setContextContent(null)
    }).finally(() => setContextLoading(false))
  }, [channelName, members, agents])

  const agentMembers = members.filter((m) => m.memberType === 'agent')
  const channelAgents = agentMembers
    .map((m) => agents.find((a) => a.id === m.memberId))
    .filter(Boolean) as typeof agents

  const taskCounts = tasks.reduce<Record<string, number>>((acc, t) => {
    acc[t.status] = (acc[t.status] || 0) + 1
    return acc
  }, {})
  const totalTasks = tasks.length
  const openTasks = totalTasks - (taskCounts['done'] || 0)

  return (
    <div className="space-y-5">
      {/* Agents */}
      <div>
        <SectionHeader title={`Agents (${channelAgents.length})`} className="mb-2" />
        {channelAgents.length === 0 ? (
          <p className="text-xs text-muted-foreground">No agents in this channel</p>
        ) : (
          <div className="space-y-1.5">
            {channelAgents.map((agent) => (
              <div key={agent.id} className="flex items-center gap-2 px-2 py-1.5 rounded-lg hover:bg-accent/50">
                <Bot className="h-4 w-4 text-primary shrink-0" />
                <span className="text-sm flex-1 truncate">@{agent.name}</span>
                <div className="flex items-center gap-1">
                  <StatusDot status={(agent.status as 'online' | 'offline' | 'working' | 'error') || 'offline'} size="sm" />
                  <span className="text-[10px] text-muted-foreground capitalize">{agentStatusLabel(agent.status)}</span>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Tasks Summary */}
      <div>
        <SectionHeader
          title={`Tasks (${openTasks} open / ${totalTasks} total)`}
          className="mb-2"
          action={<ListTodo className="h-3.5 w-3.5 text-muted-foreground" />}
        />
        {totalTasks === 0 ? (
          <p className="text-xs text-muted-foreground">No tasks in this channel</p>
        ) : (
          <div className="space-y-1.5">
            {(['todo', 'in_progress', 'in_review', 'done'] as const).map((status) => {
              const count = taskCounts[status] || 0
              const pct = totalTasks > 0 ? (count / totalTasks) * 100 : 0
              return (
                <div key={status} className="flex items-center gap-2 text-xs">
                  <Badge variant={taskStatusVariant(status)} size="sm" className="w-20 justify-center shrink-0">
                    {taskStatusLabel(status)}
                  </Badge>
                  <div className="flex-1 h-2 rounded-full bg-muted overflow-hidden">
                    <div
                      className={cn('h-full rounded-full transition-all', {
                        'bg-gray-400': status === 'todo',
                        'bg-warning': status === 'in_progress',
                        'bg-info': status === 'in_review',
                        'bg-success': status === 'done',
                      })}
                      style={{ width: `${pct}%` }}
                    />
                  </div>
                  <span className="w-6 text-right text-muted-foreground">{count}</span>
                </div>
              )
            })}
          </div>
        )}
      </div>

      {/* Recent Activity */}
      <div>
        <SectionHeader title="Recent Agent Activity" className="mb-2" />
        {channelAgents.filter((a) => a.status !== 'offline').length === 0 ? (
          <p className="text-xs text-muted-foreground">No active agents</p>
        ) : (
          <div className="space-y-2">
            {channelAgents
              .filter((a) => a.status !== 'offline')
              .map((agent) => (
                <div key={agent.id} className="rounded-lg border p-2 text-xs space-y-1">
                  <div className="flex items-center gap-1.5 font-medium">
                    <Bot className="h-3 w-3 text-primary" />
                    @{agent.name}
                  </div>
                  {agent.detail && (
                    <p className="text-muted-foreground truncate">{agent.detail}</p>
                  )}
                </div>
              ))}
          </div>
        )}
      </div>

      {/* Threads */}
      <div>
        <SectionHeader
          title={`Threads (${threads.length})`}
          className="mb-2"
          action={<MessageSquare className="h-3.5 w-3.5 text-muted-foreground" />}
        />
        {threads.length === 0 ? (
          <p className="text-xs text-muted-foreground">No threads in this channel</p>
        ) : (
          <div className="space-y-1">
            {threads.map((thread) => (
              <button
                key={thread.id}
                onClick={() => {
                  // Create a minimal parent message to open the thread
                  const parentMsg: Message = {
                    id: thread.parentMessageId || '',
                    channelId,
                    senderType: 'user',
                    senderName: '',
                    content: '',
                    seq: 0,
                    createdAt: thread.createdAt,
                  }
                  onOpenThread(thread, parentMsg)
                }}
                className="w-full flex items-center gap-2 px-2 py-1.5 rounded-lg hover:bg-accent/50 text-left transition-colors"
              >
                <MessageSquare className="h-3.5 w-3.5 text-muted-foreground shrink-0" />
                <span className="text-xs flex-1 truncate">{thread.name}</span>
                <span className="text-[10px] text-muted-foreground">
                  {new Date(thread.createdAt).toLocaleDateString()}
                </span>
              </button>
            ))}
          </div>
        )}
      </div>

      {/* Channel Context */}
      <div>
        <SectionHeader
          title="Agent Context"
          className="mb-2"
          action={<FileText className="h-3.5 w-3.5 text-muted-foreground" />}
        />
        {contextLoading ? (
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <Loader2 className="h-3 w-3 animate-spin" /> Loading context...
          </div>
        ) : contextContent ? (
          <div className="space-y-1.5">
            {contextAgent && (
              <p className="text-[10px] text-muted-foreground">from @{contextAgent}'s workspace</p>
            )}
            <pre className="text-xs whitespace-pre-wrap break-words bg-muted/50 rounded-lg p-3 max-h-48 overflow-y-auto font-mono leading-relaxed">
              {contextContent}
            </pre>
          </div>
        ) : (
          <p className="text-xs text-muted-foreground">No context file yet</p>
        )}
      </div>
    </div>
  )
}

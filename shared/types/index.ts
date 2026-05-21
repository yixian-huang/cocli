export interface User {
  id: string
  name: string
  displayName?: string
}

export interface Channel {
  id: string
  name: string
  displayName?: string
  type: 'channel' | 'dm' | 'thread'
  description?: string
  parentId?: string
  parentMessageId?: string
  createdBy?: string
  createdAt: string
  memberCount?: number
  unreadCount?: number
  done?: boolean
  archived?: boolean
}

export interface Message {
  id: string
  channelId: string
  senderId?: string
  senderType: 'user' | 'agent' | 'system'
  senderName: string
  messageType?: 'chat' | 'system'
  content: string
  seq: number
  blocks?: { type: string; data: Record<string, unknown> }[]
  attachments?: unknown[]
  pinned?: boolean
  pinnedBy?: string
  pinnedAt?: string
  createdAt: string
  // Task fields (populated from server JOIN)
  taskNumber?: number
  taskStatus?: string
  taskAssignee?: string
  taskClaimedAt?: string
  taskCompletedAt?: string
  hideFromAgents?: boolean
  hiddenFromAgents?: boolean
  visibility?: 'public' | 'private'
  priorityClass?: PriorityClass
}

export type AgentStatus = 'offline' | 'online' | 'working' | 'error'
export type AgentAttentionState =
  | 'idle'
  | 'working'
  | 'focus'
  | 'preempting'
  | 'stalled'
  | 'context_pressure'
  | 'context_overflow'
  | 'backstop_threshold_adjusted'
  | 'rate_limited'
export type PriorityClass = 'critical' | 'high' | 'normal' | 'low'
export type ResponderRole = 'owner' | 'backup' | 'observer' | 'silent'
export type ResponderMode = 'collaborative' | 'governed' | 'strict'

export interface Agent {
  id: string
  name: string
  displayName?: string
  description?: string
  runtime: string
  model: string
  workingRuntime?: string
  workingModel?: string
  chatOnly?: boolean
  status: AgentStatus
  attentionState?: AgentAttentionState
  focusTaskId?: string
  focusScope?: string
  focusSince?: number
  priorityClass?: PriorityClass
  preempted?: boolean
  sessionId?: string
  createdAt: string
  updatedAt: string
  activity?: string
  detail?: string
  trajectory?: string[]
  errorDetail?: string
  // Token usage (updated on each turn_end via WebSocket)
  lastInputTokens?: number
  totalOutputTokens?: number
  contextWindow?: number
  totalCostUSD?: number
  turnCount?: number
}

export interface OverflowRecentEvent {
  utilPct: number
  occurredAt: string
  sessionAgeSeconds: number
  contextWindowTokens?: number
}

export interface OverflowStatsEntry {
  driver: string
  model: string
  currentBackstopPct: number
  overflowCount: number
  recentOverflows: OverflowRecentEvent[]
  forksSinceLastOverflow: number
  lastAdjustedAt?: string
  contextWindowTokens: number
  defaultBackstopPct: number
}

export interface TrajectoryEntry {
  kind: 'input' | 'thinking' | 'text' | 'tool_call' | 'tool_result' | 'status' | 'warning' | 'error'
  id?: string
  text?: string
  input?: Record<string, unknown>
  result?: string
  error?: string
  ts?: number
}

export interface Turn {
  id: string
  agentId: string
  sessionId: string
  launchId?: string
  turnNumber: number
  startedAt: string
  endedAt?: string
  inputTokens?: number
  outputTokens?: number
  costUsd?: number
  contextWindow?: number
  channelName?: string
  contextUsagePct?: number
  entries: TrajectoryEntry[]
  durationMs?: number
  messageRef?: {
    channelId: string
    messageId: string
    seq?: number
    createdAt?: string
  }
  toolCalls?: {
    id?: string
    name: string
    status?: 'pending' | 'success' | 'error'
    durationMs?: number
    inputSummary?: string
    outputSummary?: string
  }[]
}

export interface HistoryMessage {
  id: string
  channelId: string
  channelName?: string
  senderId?: string
  senderType: 'user' | 'agent' | 'system'
  senderName: string
  senderDisplayName?: string
  content: string
  seq: number
  createdAt: string
  hiddenFromAgents?: boolean
}

export interface HistoryQuery {
  channelId?: string
  q?: string
  from?: string
  to?: string
  senderType?: 'user' | 'agent' | 'system'
  senderId?: string
  page?: number
  pageSize?: number
}

export interface HistoryResult {
  items: HistoryMessage[]
  page: number
  pageSize: number
  total: number
}

export type TaskStatus = 'pending' | 'claimed' | 'in_progress' | 'completed' | 'failed'

export interface Task {
  id: string
  channelId: string
  messageId?: string
  taskNumber: number
  title: string
  status: TaskStatus
  progress?: string
  assigneeId?: string
  assigneeType?: string
  assigneeName?: string
  createdById?: string
  createdByType?: string
  createdAt: string
  updatedAt: string
  executionIntentId?: string
  executionIntentStatus?: ExecutionIntentStatus
  executionRunId?: string
  executionRunStatus?: ExecutionRunStatus
}

export type ExecutionIntentStatus = 'pending' | 'running' | 'completed' | 'failed' | 'canceled'
export type ExecutionRunStatus = 'running' | 'succeeded' | 'failed' | 'canceled'

export interface TaskExecutionIntent {
  id: string
  taskId: string
  sourceMessageId?: string
  scope: string
  status: ExecutionIntentStatus
  createdAt: string
  updatedAt: string
}

export interface TaskExecutionRun {
  id: string
  intentId: string
  agentSessionId?: string
  status: ExecutionRunStatus
  startedAt: string
  endedAt?: string
  summary?: string
}

export interface AgentActivityEntry {
  id: string
  agentId: string
  activity: string
  detail?: string
  trajectory?: string[]
  launchId?: string
  createdAt: string
  sessionRowId?: string
  sessionId?: string
  executionTaskId?: string
  executionIntentId?: string
  executionIntentStatus?: ExecutionIntentStatus
  executionRunId?: string
  executionRunStatus?: ExecutionRunStatus
}

export interface AgentSession {
  id: string
  agentId: string
  sessionId: string
  launchId?: string
  channelId?: string
  parentSessionId?: string
  endReason?: string
  turnCount: number
  inputTokens: number
  outputTokens: number
  costUsd: number
  contextWindow: number
  sessionType: string
  scope?: string
  parentChatSessionId?: string
  taskSummary?: string
  filesChanged?: string[]
  taskSuccess?: boolean | null
  startedAt: string
  endedAt?: string
  executionTaskId?: string
  executionIntentId?: string
  executionIntentStatus?: ExecutionIntentStatus
  executionRunId?: string
  executionRunStatus?: ExecutionRunStatus
}

export interface TaskExecutionRunTimeline extends TaskExecutionRun {
  agentSession?: AgentSession
  activities?: AgentActivityEntry[]
}

export interface TaskExecutionIntentTimeline extends TaskExecutionIntent {
  runs: TaskExecutionRunTimeline[]
}

export interface TaskExecutionTimeline {
  task: Task
  intents: TaskExecutionIntentTimeline[]
}

export interface BookmarkEntry {
  bookmarkId: string
  message: Message
  channelName: string
  createdAt: string
}

export interface ThreadSummary {
  id: string
  parentMessage: Message
  parentChannelName: string
  replyCount: number
  lastActivityAt: string
  done: boolean
}


export interface ChannelResponderPolicy {
  channelId: string
  agentId: string
  role: ResponderRole
  priorityWeight: number
  updatedAt: string
}

export interface ChannelResponderModeState {
  channelId: string
  mode: ResponderMode
  updatedAt?: string
}

// WebSocket event from server — all payloads arrive in the `data` field.
export interface WSEvent {
  type: string
  data?: unknown
}

// Plugins (cocli OSS spec §4.1 + §4.4)
export type PluginCapability = 'inbound-bridge' | 'outbound-bridge'

export interface Plugin {
  id: string
  name: string
  capabilities: PluginCapability[]
  createdAt: string
  lastSeenAt: string | null
}

export interface PluginRegistration {
  plugin: Plugin
  token: string  // plaintext; server returns ONCE per spec §4.4
}

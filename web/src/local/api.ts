export interface RuntimeInfo {
  name: string
  installed: boolean
  binary: string | null
  version: string | null
  models: string[]
  capabilities: string[]
  unavailable_reason: string | null
}

export interface Channel {
  id: string
  name: string
  description: string | null
  goal: string | null
  kind: 'standard' | 'direct'
  is_system: boolean
  direct_agent_id: string | null
  created_by_agent_id: string | null
  created_by_channel_id: string | null
  created_at: string
}

export type AgentStatus = 'running' | 'stopped'
export type AgentLifecycleStatus = 'active' | 'paused' | 'archived'

export interface Agent {
  id: string
  name: string
  description: string | null
  instructions: string | null
  runtime: string
  model: string | null
  status: AgentStatus
  lifecycle_status: AgentLifecycleStatus
  created_by_agent_id: string | null
  created_by_channel_id: string | null
  created_at: string
}

export interface ChannelAgent {
  channel_id: string
  agent_id: string
  role: string | null
  delivery_policy: 'subscribed' | 'muted'
  joined_at: string
  created_by_agent_id: string | null
  created_by_channel_id: string | null
}

export type BuiltInWorkspaceProviderKey = 'managed' | 'directory' | 'git' | 'external'

export interface Workspace {
  id: string
  provider_key: string
  descriptor_version: number
  display_name: string
  portable_locator: string | null
  metadata: Record<string, unknown>
  created_at: string
  updated_at: string
  owner_type?: 'agent' | 'channel' | null
  owner_id?: string | null
  kind?: string | null
  locator?: string | null
}

export interface AgentOperation {
  id: string
  caller_agent_id: string
  action: string
  idempotency_key: string | null
  request_fingerprint: string
  result_type: 'agent' | 'channel' | 'membership'
  result_id: string
  source_channel_id: string | null
  source_session_id: string | null
  created_at: string
}

export interface WorkingState {
  agent_id: string
  summary: string
  channel_name: string | null
  task_number: number | null
  next_step_hint: string | null
  started_at: string
  updated_at: string
}

export interface Message {
  id: string
  channel_id?: string
  seq: number
  agent_id: string | null
  role: 'user' | 'assistant'
  content: string
  created_at: string
}

export interface LiveEvent {
  kind: string
  channelId: string | null
  agentId: string | null
  messageId: string | null
  payload: Record<string, unknown>
  occurredAt: string
}

export type LiveConnectionState = 'connecting' | 'connected' | 'reconnecting' | 'unavailable'

export interface RuntimeSessionStatus {
  agent_id: string
  running: boolean
  active_turn: boolean
  supports_turn_cancel: boolean
  supports_turn_steer: boolean
  supports_thread_fork: boolean
}

export interface GlobalSearchResult {
  kind: 'channel' | 'agent' | 'message' | 'task'
  id: string
  title: string
  snippet: string
  channelId: string | null
  agentId: string | null
  messageId: string | null
  taskNumber: number | null
  path: string | null
}

export type RuntimeSkillCompatibility = 'supported' | 'uncertain' | 'unsupported' | 'unknown'

export interface RuntimeSkillEvidence {
  source: string
  detail: string
  provesSessionVisibility: boolean
}

export interface RuntimeSkillIssue {
  fingerprint: string
  code: string
  severity: 'warning' | 'error'
  message: string
  path?: string
  skillName?: string
  relatedPaths?: string[]
  relatedCodes?: string[]
}

export interface RuntimeSkillSearchPath {
  path: string
  scope: 'workspace' | 'user'
  exists: boolean
  readable: boolean
  symlink: boolean
  resolvedPath?: string
  issue?: string
}

export interface SkillLibraryEntry {
  id: string
  zoneId: string
  name: string
  displayName: string
  description: string
  userInvocable: boolean
  sourceKind: 'git' | 'local'
  sourceUrl: string
  sourceSubpath?: string
  sourceRef?: string
  totalBytes: number
  fileCount: number
  importedBy: string
  importedAt: string
  updatedAt: string
  inUseCount: number
}

export interface AgentSkill {
  fingerprint: string
  name: string
  displayName: string
  description: string
  userInvocable: boolean
  type: 'global' | 'user' | 'workspace'
  path?: string
  installPath?: string
  state: 'managed' | 'external' | 'broken'
  presence: 'installed' | 'discovered'
  runtime: string
  scope: 'workspace' | 'user' | 'global'
  sourcePath: string
  resolvedPath?: string
  evidence: RuntimeSkillEvidence
  enabled?: boolean
  valid?: boolean
  duplicate: boolean
  shadowed: boolean
  issues: RuntimeSkillIssue[]
  installId?: string
  libraryId?: string
  sourceUrl?: string
  sourceRef?: string
}

export interface AgentSkillInventory {
  observedAt: string
  cacheStatus: SkillSnapshotStatus
  expiresAt: string
  agentId: string
  agentName: string
  runtime: string
  compatibility: RuntimeSkillCompatibility
  evidence: RuntimeSkillEvidence
  searchPaths: RuntimeSkillSearchPath[]
  skills: AgentSkill[]
  issues: RuntimeSkillIssue[]
}

export interface RuntimeSkillInventorySummary {
  observedAt: string
  cacheStatus: SkillSnapshotStatus
  expiresAt: string
  runtime: string
  compatibility: RuntimeSkillCompatibility
  agentCount: number
  skillCount: number
  issueCount: number
  evidenceSources: string[]
  evidence: RuntimeSkillEvidence
  searchPaths: RuntimeSkillSearchPath[]
  skills: AgentSkill[]
  issues: RuntimeSkillIssue[]
}

export type SkillSnapshotStatus = 'fresh' | 'cached' | 'mixed'

export interface SkillInspectionDiagnostic {
  fingerprint: string
  subject: 'runtime' | 'agent'
  runtime: string
  agentId?: string
  agentName?: string
  stage: string
  errorType: string
  message: string
  observedAt: string
}

export interface SkillDoctorSummary {
  status: 'ok' | 'warning' | 'error'
  runtimeCount: number
  agentCount: number
  skillCount: number
  issueCount: number
  errorCount: number
  warningCount: number
}

export interface MachineSkillDoctor {
  observedAt: string
  cacheStatus: SkillSnapshotStatus
  forceRefresh: boolean
  summary: SkillDoctorSummary
  runtimes: RuntimeSkillInventorySummary[]
  agents: AgentSkillInventory[]
  diagnostics: SkillInspectionDiagnostic[]
}

export interface SkillFileEntry {
  name: string
  isDir: boolean
  size: number
}

export type TaskStatus = 'todo' | 'in_progress' | 'in_review' | 'done'

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
}

export type MemoryScope = 'agent' | 'channel'
export type MemoryType = 'user' | 'feedback' | 'project' | 'reference'

export interface MemoryDocumentEntry {
  path: string
  body: string
  version: number
}

export interface MemoryTopic {
  type: MemoryType
  topic: string
  description: string
  updated: string
  body: string
  path: string
  version: number
}

export interface RuntimeSession {
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
}

export interface RuntimeTrajectoryEntry {
  kind: 'input' | 'thinking' | 'text' | 'tool_call' | 'tool_result' | 'status' | 'warning' | 'error'
  id?: string
  text?: string
  input?: Record<string, unknown>
  result?: string
  error?: string
  ts?: number
}

export interface RuntimeTurn {
  id: string
  agentId: string
  sessionId: string
  launchId?: string
  turnNumber: number
  startedAt: string
  endedAt?: string
  inputTokens: number
  outputTokens: number
  costUsd: number
  contextWindow: number
  entries: RuntimeTrajectoryEntry[]
  sessionType: string
  durationMs?: number
  messageRef?: {
    channelId: string
    messageId: string
    seq?: number
    createdAt?: string
  }
}

export interface RuntimeActivity {
  id: string
  agentId: string
  activity: string
  detail?: string
  trajectory: string[]
  launchId?: string
  createdAt: string
  sessionRowId?: string
  sessionId?: string
}

interface PostMessageResponse {
  message: Message
  replies: Message[]
  pending_deliveries?: Array<{
    id: string
    state: 'pending' | 'in_flight' | 'exhausted'
    attempts: number
  }>
}

interface ApiErrorBody {
  error?: string
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(path, {
    ...init,
    headers: {
      'Content-Type': 'application/json',
      ...init?.headers,
    },
  })

  if (!response.ok) {
    let message = `${response.status} ${response.statusText}`.trim()
    try {
      const body = await response.json() as ApiErrorBody
      if (body.error) message = body.error
    } catch {
      // Keep the HTTP status when the server did not return JSON.
    }
    throw new Error(message)
  }

  return response.json() as Promise<T>
}

export const localApi = {
  globalSearch: (query: string) =>
    request<{ results: GlobalSearchResult[] }>(`/api/search?q=${encodeURIComponent(query)}`),
  listRuntimes: () => request<RuntimeInfo[]>('/api/runtimes'),
  listChannels: () => request<Channel[]>('/api/channels'),
  createChannel: (input: { name: string; description?: string; goal?: string }) =>
    request<Channel>('/api/channels', {
      method: 'POST',
      body: JSON.stringify(input),
    }),
  listAgents: () => request<Agent[]>('/api/agents'),
  createAgent: (input: {
    channel_id?: string
    name: string
    description?: string
    instructions?: string
    runtime: string
    model: string | null
  }) =>
    request<Agent>('/api/agents', {
      method: 'POST',
      body: JSON.stringify(input),
    }),
  listAgentChannels: (agentId: string) =>
    request<Channel[]>(`/api/agents/${agentId}/channels`),
  listAgentMessages: (agentId: string) =>
    request<Message[]>(`/api/agents/${agentId}/messages`),
  postAgentMessage: (agentId: string, content: string) =>
    request<PostMessageResponse>(`/api/agents/${agentId}/messages`, {
      method: 'POST',
      body: JSON.stringify({ content }),
    }),
  listAgentOperations: (agentId: string) =>
    request<AgentOperation[]>(`/api/agents/${agentId}/operations`),
  getAgentWorkingState: (agentId: string) =>
    request<WorkingState | null>(`/api/agents/${agentId}/working`),
  listChannelMembers: (channelId: string) =>
    request<Agent[]>(`/api/channels/${channelId}/agents`),
  addChannelMember: (
    channelId: string,
    input: { agent_id: string; role?: string; delivery_policy?: 'subscribed' | 'muted' },
  ) =>
    request<ChannelAgent>(`/api/channels/${channelId}/agents`, {
      method: 'POST',
      body: JSON.stringify(input),
    }),
  listChannelWorkspaces: (channelId: string) =>
    request<Workspace[]>(`/api/channels/${channelId}/workspaces`),
  attachChannelWorkspace: (
    channelId: string,
    input: { kind: BuiltInWorkspaceProviderKey; locator?: string; metadata?: Record<string, unknown> },
  ) =>
    request<Workspace>(`/api/channels/${channelId}/workspaces`, {
      method: 'POST',
      body: JSON.stringify(input),
    }),
  listAgentWorkspaces: (agentId: string) =>
    request<Workspace[]>(`/api/agents/${agentId}/workspaces`),
  attachAgentWorkspace: (
    agentId: string,
    input: { kind: BuiltInWorkspaceProviderKey; locator?: string; metadata?: Record<string, unknown> },
  ) =>
    request<Workspace>(`/api/agents/${agentId}/workspaces`, {
      method: 'POST',
      body: JSON.stringify(input),
    }),
  setAgentStatus: (agentId: string, status: AgentStatus) =>
    request<Agent>(`/api/agents/${agentId}/${status === 'running' ? 'start' : 'stop'}`, {
      method: 'POST',
    }),
  getRuntimeStatus: (agentId: string) =>
    request<RuntimeSessionStatus>(`/api/agents/${agentId}/runtime`),
  cancelTurn: (agentId: string) =>
    request<{ ok: boolean }>(`/api/agents/${agentId}/turn/cancel`, {
      method: 'POST',
    }),
  listMessages: (channelId: string) =>
    request<Message[]>(`/api/channels/${channelId}/messages`),
  postMessage: (channelId: string, content: string) =>
    request<PostMessageResponse>(`/api/channels/${channelId}/messages`, {
      method: 'POST',
      body: JSON.stringify({ content }),
    }),
  subscribeToEvents: (
    onEvent: (event: LiveEvent) => void,
    onConnectionState?: (state: LiveConnectionState) => void,
  ) => {
    if (typeof EventSource === 'undefined') {
      onConnectionState?.('unavailable')
      return () => undefined
    }
    onConnectionState?.('connecting')
    const source = new EventSource('/api/events')
    source.onopen = () => onConnectionState?.('connected')
    source.onerror = () => onConnectionState?.('reconnecting')
    source.onmessage = (message) => {
      try {
        onEvent(JSON.parse(message.data) as LiveEvent)
      } catch {
        // Ignore malformed transient events; durable state remains reloadable.
      }
    }
    return () => source.close()
  },
  listSkillCompatibility: () =>
    request<Record<string, RuntimeSkillCompatibility>>('/api/runtimes/compatibility'),
  inspectMachineSkills: (force = false) =>
    request<MachineSkillDoctor>(`/api/runtimes/skills/doctor${force ? '?force=true' : ''}`),
  inspectAgentSkills: (agentId: string, force = false) =>
    request<{ summary: SkillDoctorSummary; inventory: AgentSkillInventory }>(
      `/api/agents/${agentId}/skills/doctor${force ? '?force=true' : ''}`,
    ),
  listSkillLibrary: () =>
    request<{ entries: SkillLibraryEntry[] }>('/api/zones/local/skills/library'),
  importSkillLibrary: (input: { url: string; subPath?: string; name?: string }) =>
    request<{ library_id: string; files: number; size: number }>(
      '/api/zones/local/skills/library',
      {
        method: 'POST',
        body: JSON.stringify(input),
      },
    ),
  reinstallSkillLibrary: (libraryId: string) =>
    request<{ updated: boolean; source_ref?: string; files: number; size: number }>(
      `/api/zones/local/skills/library/${libraryId}/reinstall`,
      { method: 'POST' },
    ),
  deleteSkillLibrary: (libraryId: string) =>
    request<{ deleted: string }>(`/api/zones/local/skills/library/${libraryId}`, {
      method: 'DELETE',
    }),
  listAgentSkills: (agentId: string) =>
    request<{ skills: AgentSkill[] }>(`/api/agents/${agentId}/skills`),
  installAgentSkill: (agentId: string, libraryId: string) =>
    request<{ installId: string; installPath: string; bytes: number }>(
      `/api/agents/${agentId}/skills`,
      {
        method: 'POST',
        body: JSON.stringify({ libraryId }),
      },
    ),
  uninstallAgentSkill: (agentId: string, installId: string) =>
    request<{ ok: boolean }>(`/api/agents/${agentId}/skills/${installId}`, {
      method: 'DELETE',
    }),
  listAgentSkillFiles: (agentId: string, installId: string) =>
    request<{ installPath: string; files: SkillFileEntry[] }>(
      `/api/agents/${agentId}/skills/${installId}/files`,
    ),
  readAgentSkillFile: (agentId: string, installId: string, relativePath: string) =>
    request<{ content: string; binary: boolean }>(
      `/api/agents/${agentId}/skills/${installId}/files/${encodeURIComponent(relativePath)}`,
    ),
  listTasks: (channelId: string, status?: TaskStatus) =>
    request<Task[]>(
      `/api/channels/${channelId}/tasks${status ? `?status=${encodeURIComponent(status)}` : ''}`,
    ),
  createTask: (channelId: string, title: string) =>
    request<Task>(`/api/channels/${channelId}/tasks`, {
      method: 'POST',
      body: JSON.stringify({ title }),
    }),
  claimTask: (channelId: string, taskNumber: number, agentId: string) =>
    request<Task>(`/api/channels/${channelId}/tasks/${taskNumber}/claim`, {
      method: 'POST',
      body: JSON.stringify({ agentId }),
    }),
  unclaimTask: (channelId: string, taskNumber: number) =>
    request<Task>(`/api/channels/${channelId}/tasks/${taskNumber}/unclaim`, {
      method: 'POST',
    }),
  updateTaskStatus: (
    channelId: string,
    taskNumber: number,
    status: TaskStatus,
    progress?: string,
  ) =>
    request<Task>(`/api/channels/${channelId}/tasks/${taskNumber}/status`, {
      method: 'POST',
      body: JSON.stringify({ status, progress }),
    }),
  getTaskDependencies: (channelId: string, taskNumber: number) =>
    request<{ taskNumber: number; dependsOn: number[] }>(
      `/api/channels/${channelId}/tasks/${taskNumber}/dependencies`,
    ),
  addTaskDependency: (channelId: string, taskNumber: number, dependsOn: number) =>
    request<{ taskNumber: number; dependsOn: number[] }>(
      `/api/channels/${channelId}/tasks/${taskNumber}/dependencies`,
      {
        method: 'POST',
        body: JSON.stringify({ dependsOn }),
      },
    ),
  removeTaskDependency: (channelId: string, taskNumber: number, dependsOn: number) =>
    request<{ taskNumber: number; dependsOn: number[] }>(
      `/api/channels/${channelId}/tasks/${taskNumber}/dependencies`,
      {
        method: 'DELETE',
        body: JSON.stringify({ dependsOn }),
      },
    ),
  listMemory: (agentId: string, scope: MemoryScope, channelId?: string) => {
    const params = new URLSearchParams({ scope })
    if (channelId) params.set('channel_id', channelId)
    return request<{ entries: MemoryDocumentEntry[] }>(
      `/api/bridge/agents/${agentId}/memory/list?${params}`,
    )
  },
  getMemoryTopic: (
    agentId: string,
    scope: MemoryScope,
    type: MemoryType,
    topic: string,
    channelId?: string,
  ) => {
    const params = new URLSearchParams({ scope, type, topic })
    if (channelId) params.set('channel_id', channelId)
    return request<MemoryTopic>(
      `/api/bridge/agents/${agentId}/memory/topic?${params}`,
    )
  },
  writeMemoryTopic: (
    agentId: string,
    input: {
      scope: MemoryScope
      channelId?: string
      type: MemoryType
      topic: string
      description: string
      body: string
      ifVersion?: number
    },
  ) =>
    request<MemoryTopic>(`/api/bridge/agents/${agentId}/memory/topic`, {
      method: 'POST',
      body: JSON.stringify(input),
    }),
  moveMemoryTopic: (
    agentId: string,
    input: {
      fromScope: MemoryScope
      fromChannelId?: string
      toScope: MemoryScope
      toChannelId?: string
      type: MemoryType
      topic: string
    },
  ) =>
    request<{ from: string; to: string }>(
      `/api/bridge/agents/${agentId}/memory/move`,
      {
        method: 'POST',
        body: JSON.stringify({
          from_scope: input.fromScope,
          from_channel_id: input.fromChannelId,
          to_scope: input.toScope,
          to_channel_id: input.toChannelId,
          type: input.type,
          topic: input.topic,
        }),
      },
    ),
  listRuntimeSessions: (agentId: string, type?: string) => {
    const params = new URLSearchParams({ limit: '50' })
    if (type) params.set('type', type)
    return request<RuntimeSession[]>(`/api/agents/${agentId}/sessions?${params}`)
  },
  getCurrentRuntimeSession: (agentId: string) =>
    request<RuntimeSession | null>(`/api/agents/${agentId}/sessions/current`),
  listRuntimeTurns: (agentId: string, sessionId?: string) => {
    const params = new URLSearchParams({ limit: '120', offset: '0' })
    if (sessionId) params.set('sessionId', sessionId)
    return request<RuntimeTurn[]>(`/api/agents/${agentId}/turns?${params}`)
  },
  listRuntimeActivity: (agentId: string) =>
    request<RuntimeActivity[]>(`/api/agents/${agentId}/activity?limit=100&offset=0`),
}

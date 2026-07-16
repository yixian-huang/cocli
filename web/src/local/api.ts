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
  created_at: string
}

export type AgentStatus = 'running' | 'stopped'

export interface Agent {
  id: string
  channel_id: string
  name: string
  runtime: string
  model: string | null
  status: AgentStatus
  created_at: string
}

export interface Message {
  id: string
  channel_id: string
  seq: number
  agent_id: string | null
  role: 'user' | 'assistant'
  content: string
  created_at: string
}

export type RuntimeSkillCompatibility = 'supported' | 'uncertain' | 'unsupported' | 'unknown'

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
  name: string
  displayName: string
  description: string
  userInvocable: boolean
  type: 'global' | 'workspace'
  path?: string
  installPath?: string
  state: 'managed' | 'external' | 'broken'
  installId?: string
  libraryId?: string
  sourceUrl?: string
  sourceRef?: string
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

export interface WikiPage {
  id: string
  path: string
  title: string
  content: string
  tags: string[]
  version: number
  createdAt: string
  updatedAt: string
  updatedBy?: string
}

export interface WikiPageSummary {
  path: string
  title: string
  tags: string[]
  version: number
  updatedAt: string
  updatedBy?: string
}

export interface WikiRevision {
  version: number
  title: string
  content: string
  tags: string[]
  createdAt: string
  createdBy?: string
  reason?: string
}

export interface WikiBacklink {
  path: string
  title: string
  updatedAt: string
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
  listRuntimes: () => request<RuntimeInfo[]>('/api/runtimes'),
  listChannels: () => request<Channel[]>('/api/channels'),
  createChannel: (name: string) =>
    request<Channel>('/api/channels', {
      method: 'POST',
      body: JSON.stringify({ name }),
    }),
  listAgents: () => request<Agent[]>('/api/agents'),
  createAgent: (input: {
    channel_id: string
    name: string
    runtime: string
    model: string | null
  }) =>
    request<Agent>('/api/agents', {
      method: 'POST',
      body: JSON.stringify(input),
    }),
  setAgentStatus: (agentId: string, status: AgentStatus) =>
    request<Agent>(`/api/agents/${agentId}/${status === 'running' ? 'start' : 'stop'}`, {
      method: 'POST',
    }),
  listMessages: (channelId: string) =>
    request<Message[]>(`/api/channels/${channelId}/messages`),
  postMessage: (channelId: string, content: string) =>
    request<PostMessageResponse>(`/api/channels/${channelId}/messages`, {
      method: 'POST',
      body: JSON.stringify({ content }),
    }),
  listSkillCompatibility: () =>
    request<Record<string, RuntimeSkillCompatibility>>('/api/runtimes/compatibility'),
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
  listWikiPages: (query?: string, tag?: string) => {
    const params = new URLSearchParams()
    if (query) params.set('q', query)
    if (tag) params.set('tag', tag)
    params.set('limit', '200')
    return request<{ pages: WikiPageSummary[] }>(`/api/wiki/pages?${params}`)
  },
  getWikiPage: (path: string) =>
    request<WikiPage>(`/api/wiki/pages/${encodeURIComponent(path)}`),
  upsertWikiPage: (
    path: string,
    input: {
      title: string
      content: string
      tags: string[]
      updatedBy?: string
      reason?: string
      ifVersion?: number
    },
  ) =>
    request<WikiPage>(`/api/wiki/pages/${encodeURIComponent(path)}`, {
      method: 'PUT',
      body: JSON.stringify(input),
    }),
  listWikiRevisions: (path: string) =>
    request<{ revisions: WikiRevision[] }>(
      `/api/wiki/pages/${encodeURIComponent(path)}/revisions`,
    ),
  listWikiBacklinks: (path: string) =>
    request<{ backlinks: WikiBacklink[] }>(
      `/api/wiki/pages/${encodeURIComponent(path)}/backlinks`,
    ),
  revertWikiPage: (path: string, version: number) =>
    request<{ page: WikiPage }>(
      `/api/wiki/pages/${encodeURIComponent(path)}/revert`,
      {
        method: 'POST',
        body: JSON.stringify({ version, updatedBy: 'local-user' }),
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

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
}

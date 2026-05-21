import type {
  Agent,
  AgentActivityEntry,
  AgentProviderBinding,
  AgentSession,
  BookmarkEntry,
  Channel,
  ChannelResponderModeState,
  ChannelResponderPolicy,
  CreateCredentialInput,
  HistoryQuery,
  HistoryResult,
  Invite,
  Machine,
  Message,
  OverflowStatsEntry,
  ResponderMode,
  ResponderRole,
  SkillFileEntry,
  SkillLibraryEntry,
  SkillLibraryFileMeta,
  SkillLibraryImportResponse,
  SkillLibraryReinstallResponse,
  SkillView,
  Task,
  TaskExecutionTimeline,
  TenantProviderKey,
  ThreadSummary,
  Turn,
  UpsertBindingInput,
  User,
  Zone,
  ZoneInvite,
  ZoneMember,
} from '@shared/types'
import { storageKey } from '@shared/brand'

const API_BASE = (import.meta.env.VITE_API_BASE ?? '').replace(/\/$/, '')

function buildUrl(path: string): string {
  if (/^https?:\/\//i.test(path)) return path
  return API_BASE + path
}

let apiKey = ''

export function setApiKey(key: string) {
  apiKey = key
  localStorage.setItem(storageKey('api-key'), key)
}

export function getApiKey(): string {
  if (!apiKey) {
    apiKey = localStorage.getItem(storageKey('api-key')) || ''
  }
  return apiKey
}

export class ApiError extends Error {
  readonly status: number
  readonly requestId: string
  readonly body?: string

  constructor(message: string, status: number, requestId: string, body?: string) {
    super(message)
    this.name = 'ApiError'
    this.status = status
    this.requestId = requestId
    this.body = body
  }
}

/**
 * Hook for handling 401 responses (e.g. clearing auth state and redirecting
 * to /login). Wired in main.tsx so this module stays free of router/store
 * dependencies. Only fires for authenticated requests — anonymous 401s
 * (failed login attempts, expired invite checks) bubble up to callers.
 */
let onUnauthorized: ((requestId: string) => void) | null = null

export function setUnauthorizedHandler(fn: ((requestId: string) => void) | null) {
  onUnauthorized = fn
}

/**
 * In-flight request counter for a global loading indicator. Designed to be
 * consumed via React's useSyncExternalStore (see GlobalLoadingBar).
 */
let inflight = 0
const inflightListeners = new Set<() => void>()

export function getInflight(): number {
  return inflight
}

export function subscribeInflight(fn: () => void): () => void {
  inflightListeners.add(fn)
  return () => {
    inflightListeners.delete(fn)
  }
}

function bumpInflight(delta: number) {
  inflight = Math.max(0, inflight + delta)
  inflightListeners.forEach((fn) => fn())
}

function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms))
}

async function request<T>(path: string, options: RequestInit = {}): Promise<T> {
  const maxRetries = 2
  const retryDelays = [1000, 2000]
  const requestId = crypto.randomUUID()
  const sentKey = getApiKey()

  bumpInflight(1)
  try {
    for (let attempt = 0; attempt <= maxRetries; attempt++) {
      try {
        const res = await fetch(buildUrl(path), {
          ...options,
          headers: {
            'Content-Type': 'application/json',
            'X-API-Key': sentKey,
            'X-Request-Id': requestId,
            ...options.headers,
          },
        })
        if (res.status === 401 && sentKey) {
          onUnauthorized?.(requestId)
        }
        if (!res.ok) {
          if (res.status >= 500 && attempt < maxRetries) {
            await sleep(retryDelays[attempt])
            continue
          }
          const body = await res.text()
          throw new ApiError(`${res.status}: ${body}`, res.status, requestId, body)
        }
        if (res.status === 204) {
          return undefined as T
        }
        return await res.json()
      } catch (err) {
        if (err instanceof TypeError && attempt < maxRetries) {
          // Network error (fetch throws TypeError for network failures)
          await sleep(retryDelays[attempt])
          continue
        }
        throw err
      }
    }
    throw new ApiError('Max retries exceeded', 0, requestId)
  } finally {
    bumpInflight(-1)
  }
}

// Zones
export const zones = {
  list: () => request<Zone[]>('/api/zones'),
  create: (name: string, slug: string) => request<Zone>('/api/zones', {
    method: 'POST',
    body: JSON.stringify({ name, slug }),
  }),
  get: (zoneId: string) => request<Zone>(`/api/zones/${zoneId}`),
  update: (zoneId: string, name: string) =>
    request<{ ok: boolean }>(`/api/zones/${zoneId}`, {
      method: 'PUT',
      body: JSON.stringify({ name }),
    }),
  delete: (zoneId: string) =>
    request<{ ok: boolean }>(`/api/zones/${zoneId}`, { method: 'DELETE' }),
  setTheme: (zoneId: string, themeId: string) =>
    request<{ themeId: string }>(`/api/zones/${zoneId}/theme`, {
      method: 'PATCH',
      body: JSON.stringify({ themeId }),
    }),
}

// Zone Members
export const zoneMembers = {
  list: (zoneId: string) => request<ZoneMember[]>(`/api/zones/${zoneId}/members`),
  add: (zoneId: string, userId: string, role?: string) =>
    request<ZoneMember>(`/api/zones/${zoneId}/members`, {
      method: 'POST',
      body: JSON.stringify({ userId, role }),
    }),
  updateRole: (zoneId: string, userId: string, role: string) =>
    request<{ ok: boolean }>(`/api/zones/${zoneId}/members/${userId}`, {
      method: 'PUT',
      body: JSON.stringify({ role }),
    }),
  updatePermissions: (
    zoneId: string,
    userId: string,
    payload: Partial<Pick<ZoneMember, 'role' | 'canCreateChannel' | 'canCreateAgent' | 'canInviteOthers' | 'hideFromAgents'>>,
  ) =>
    request<ZoneMember>(`/api/zones/${zoneId}/members/${userId}/permissions`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    }),
  remove: (zoneId: string, userId: string) =>
    request<{ ok: boolean }>(`/api/zones/${zoneId}/members/${userId}`, { method: 'DELETE' }),
}

// Zone Daemons
export const daemons = {
  list: (zoneId: string) => request<(Machine & { connected: boolean })[]>(`/api/zones/${zoneId}/daemons`),
  create: (zoneId: string) =>
    request<{ machine: Machine; apiKey: string; installCommand: string }>(`/api/zones/${zoneId}/daemons`, { method: 'POST' }),
  installCommands: (zoneId: string, machineId: string) =>
    request<{ apiKey: string; installCommand: string }>(`/api/zones/${zoneId}/daemons/${machineId}/install`),
  remove: (zoneId: string, machineId: string) =>
    request<{ ok: boolean }>(`/api/zones/${zoneId}/daemons/${machineId}`, { method: 'DELETE' }),
  upgrade: (zoneId: string, machineId: string) =>
    request<{ status: string }>(`/api/zones/${zoneId}/daemons/${machineId}/upgrade`, { method: 'POST' }),
}

// Cocli — zone-scoped provider credential pool (PR-A).
export const chatrsCredentials = {
  list: (zoneId: string) =>
    request<TenantProviderKey[]>(`/api/zones/${zoneId}/credentials`),
  create: (zoneId: string, input: CreateCredentialInput) =>
    request<TenantProviderKey>(`/api/zones/${zoneId}/credentials`, {
      method: 'POST',
      body: JSON.stringify(input),
    }),
  remove: (zoneId: string, name: string) =>
    request<void>(
      `/api/zones/${zoneId}/credentials/${encodeURIComponent(name)}`,
      { method: 'DELETE' },
    ),
}

// Cocli — per-agent provider binding (PR-A).
export const chatrsAgentBinding = {
  get: (agentId: string) =>
    request<AgentProviderBinding>(`/api/agents/${agentId}/binding`),
  upsert: (agentId: string, input: UpsertBindingInput) =>
    request<AgentProviderBinding>(`/api/agents/${agentId}/binding`, {
      method: 'PUT',
      body: JSON.stringify(input),
    }),
  remove: (agentId: string) =>
    request<void>(`/api/agents/${agentId}/binding`, { method: 'DELETE' }),
  setWriteEnabled: (agentId: string, enabled: boolean) =>
    request<void>(`/api/agents/${agentId}/binding/write`, {
      method: 'PATCH',
      body: JSON.stringify({ enabled }),
    }),
}

// Users
export const users = {
  me: () => request<User>('/api/users/me'),
  list: (zoneId: string) => request<User[]>(`/api/zones/${zoneId}/users`),
  create: (name: string) => request<User>('/api/users', {
    method: 'POST',
    body: JSON.stringify({ name }),
  }),
  updateProfile: (displayName: string) => request<User>('/api/users/me', {
    method: 'PUT',
    body: JSON.stringify({ displayName }),
  }),
}

// Channels
export const channels = {
  list: (zoneId: string, opts?: { includeArchived?: boolean }) => {
    const q = opts?.includeArchived ? '?includeArchived=true' : ''
    return request<Channel[]>(`/api/zones/${zoneId}/channels${q}`)
  },
  create: (zoneId: string, name: string, description?: string) => request<Channel>(`/api/zones/${zoneId}/channels`, {
    method: 'POST',
    body: JSON.stringify({ name, description }),
  }),
  get: (id: string) => request<Channel>(`/api/channels/${id}`),
  update: (id: string, data: { displayName?: string; description?: string }) =>
    request<Channel>(`/api/channels/${id}`, { method: 'PUT', body: JSON.stringify(data) }),
  delete: (id: string) => request<void>(`/api/channels/${id}`, { method: 'DELETE' }),
  getMembers: (id: string) => request<{ id: string; memberId: string; memberType: string }[]>(`/api/channels/${id}/members`),
  addMember: (id: string, memberId: string, memberType: string) =>
    request<void>(`/api/channels/${id}/members`, { method: 'POST', body: JSON.stringify({ memberId, memberType }) }),
  removeMember: (id: string, memberId: string, memberType: string) =>
    request<void>(`/api/channels/${id}/members`, { method: 'DELETE', body: JSON.stringify({ memberId, memberType }) }),
  listResponderPolicies: (id: string) =>
    request<ChannelResponderPolicy[]>(`/api/channels/${id}/responder-policies`),
  upsertResponderPolicy: (id: string, agentId: string, role: ResponderRole, priorityWeight = 0) =>
    request<ChannelResponderPolicy>(`/api/channels/${id}/responder-policies/${agentId}`, {
      method: 'PUT',
      body: JSON.stringify({ role, priorityWeight }),
    }),
  getResponderMode: (id: string) =>
    request<ChannelResponderModeState>(`/api/channels/${id}/responder-mode`),
  updateResponderMode: (id: string, mode: ResponderMode) =>
    request<ChannelResponderModeState>(`/api/channels/${id}/responder-mode`, {
      method: 'PUT',
      body: JSON.stringify({ mode }),
    }),
  archive: (channelId: string, archived: boolean) =>
    request<{ ok: true; archived: boolean }>(`/api/channels/${channelId}/archive`, {
      method: 'PATCH',
      body: JSON.stringify({ archived }),
    }),
}

// User prefs
export const prefs = {
  get: () => request<{ prefs: Record<string, unknown> }>(`/api/users/me/prefs`),
  put: (prefs: Record<string, unknown>) =>
    request<{ ok: true }>(`/api/users/me/prefs`, {
      method: 'PUT',
      body: JSON.stringify({ prefs }),
    }),
}

// Pins
export const pins = {
  list: (channelId: string) => request<{ messages: Message[] }>(`/api/channels/${channelId}/pins`),
  pin: (messageId: string) => request<Message>(`/api/messages/${messageId}/pin`, { method: 'POST' }),
  unpin: (messageId: string) => request<Message>(`/api/messages/${messageId}/pin`, { method: 'DELETE' }),
}

// Reactions
export interface ReactionSummary {
  emoji: string
  count: number
  userIds: string[]
}

export const reactions = {
  list: (messageId: string) => request<ReactionSummary[]>(`/api/messages/${messageId}/reactions`),
  add: (messageId: string, emoji: string) =>
    request<unknown>(`/api/messages/${messageId}/reactions`, {
      method: 'POST',
      body: JSON.stringify({ emoji }),
    }),
  remove: (messageId: string, emoji: string) =>
    request<unknown>(`/api/messages/${messageId}/reactions?emoji=${encodeURIComponent(emoji)}`, {
      method: 'DELETE',
    }),
}

// Search
export const search = {
  messages: (zoneId: string, q: string, limit?: number, options?: { signal?: AbortSignal }) => {
    const qs = new URLSearchParams({ q })
    if (limit) qs.set('limit', String(limit))
    return request<{ messages: Message[] }>(`/api/zones/${zoneId}/messages/search?${qs}`, {
      signal: options?.signal,
    })
  },
}

export const history = {
  list: (zoneId: string, params: HistoryQuery = {}) => {
    const qs = new URLSearchParams()
    if (params.channelId) qs.set('channelId', params.channelId)
    if (params.q) qs.set('q', params.q)
    if (params.from) qs.set('from', params.from)
    if (params.to) qs.set('to', params.to)
    if (params.senderType) qs.set('senderType', params.senderType)
    if (params.senderId) qs.set('senderId', params.senderId)
    qs.set('page', String(params.page ?? 1))
    qs.set('pageSize', String(params.pageSize ?? 30))
    return request<HistoryResult>(`/api/zones/${zoneId}/history?${qs.toString()}`)
  },
}

// Messages
export const messages = {
  list: (channelId: string, params?: { before?: number; after?: number; limit?: number }) => {
    const qs = new URLSearchParams()
    if (params?.before) qs.set('before', String(params.before))
    if (params?.after) qs.set('after', String(params.after))
    if (params?.limit) qs.set('limit', String(params.limit))
    const query = qs.toString()
    return request<{ messages: Message[]; hasMore: boolean }>(`/api/channels/${channelId}/messages${query ? '?' + query : ''}`)
  },
  send: (channelId: string, content: string) => request<Message>(`/api/channels/${channelId}/messages`, {
    method: 'POST',
    body: JSON.stringify({ content }),
  }),
  markRead: (channelId: string, seq: number) => request<void>(`/api/channels/${channelId}/read`, {
    method: 'POST',
    body: JSON.stringify({ seq }),
  }),
  blockAction: (messageId: string, payload: { action_id: string; value?: string; form_data?: Record<string, unknown> }) =>
    request<Message>(`/api/messages/${messageId}/action`, {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
}

// DMs
export const dm = {
  list: (zoneId: string) => request<Channel[]>(`/api/zones/${zoneId}/dm`),
  createOrGet: (zoneId: string, peerName: string, peerType?: string) => request<Channel>(`/api/zones/${zoneId}/dm`, {
    method: 'POST',
    body: JSON.stringify({ peerName, peerType }),
  }),
}

// Agents
export const agents = {
  list: (zoneId: string) => request<Agent[]>(`/api/zones/${zoneId}/agents`),
  create: (zoneId: string, data: {
      name: string;
      runtime?: string;
      model?: string;
      description?: string;
      machineId?: string;
      workingRuntime?: string;
      workingModel?: string;
      chatOnly?: boolean;
  }) =>
    request<Agent>(`/api/zones/${zoneId}/agents`, { method: 'POST', body: JSON.stringify(data) }),
  get: (id: string) => request<Agent>(`/api/agents/${id}`),
  update: (id: string, data: Partial<Agent>) =>
    request<Agent>(`/api/agents/${id}`, { method: 'PUT', body: JSON.stringify(data) }),
  start: (id: string) => request<void>(`/api/agents/${id}/start`, { method: 'POST' }),
  stop: (id: string, force?: boolean) =>
    request<void>(`/api/agents/${id}/stop${force ? '?force=true' : ''}`, { method: 'POST' }),
  cancelTurn: (id: string) =>
    request<void>(`/api/agents/${id}/turn/cancel`, { method: 'POST' }),
  steerTurn: (id: string, input: string) =>
    request<void>(`/api/agents/${id}/turn/steer`, {
      method: 'POST',
      body: JSON.stringify({ input }),
    }),
  forkThread: (id: string) =>
    request<void>(`/api/agents/${id}/thread/fork`, { method: 'POST' }),
  delete: (id: string) => request<void>(`/api/agents/${id}`, { method: 'DELETE' }),
  runtimes: (zoneId: string) => request<string[]>(`/api/zones/${zoneId}/agents/runtimes`),
}

// Attachments
export const attachments = {
  upload: async (file: File): Promise<{ id: string; filename: string; url: string }> => {
    const form = new FormData()
    form.append('file', file)
    const requestId = crypto.randomUUID()
    const sentKey = getApiKey()
    bumpInflight(1)
    try {
      const res = await fetch(buildUrl('/api/attachments/upload'), {
        method: 'POST',
        headers: {
          'X-API-Key': sentKey,
          'X-Request-Id': requestId,
        },
        body: form,
      })
      if (res.status === 401 && sentKey) {
        onUnauthorized?.(requestId)
      }
      if (!res.ok) {
        const body = await res.text()
        throw new ApiError(`${res.status}: ${body}`, res.status, requestId, body)
      }
      return await res.json()
    } finally {
      bumpInflight(-1)
    }
  },
}

// Threads
export const threads = {
  getOrCreate: (channelId: string, messageId: string) =>
    request<Channel>(`/api/channels/${channelId}/messages/${messageId}/thread`, { method: 'POST' }),
  list: (channelId: string) =>
    request<Channel[]>(`/api/channels/${channelId}/threads`),
  listAll: (zoneId: string) =>
    request<{ threads: ThreadSummary[] }>(`/api/zones/${zoneId}/threads`),
  setDone: (threadId: string, done: boolean) =>
    request<{ id: string; done: boolean }>(`/api/threads/${threadId}/done`, {
      method: 'PATCH',
      body: JSON.stringify({ done }),
    }),
}

// Presence
export const presence = {
  list: () => request<{ online: string[] }>('/api/presence'),
  setViewingChannel: (channelId: string | null) =>
    request<{ ok: true }>('/api/presence/viewing', {
      method: 'POST',
      body: JSON.stringify({ channelId }),
    }),
}

// Export
export const exportData = {
  messagesUrl: (channelId: string, format: 'json' | 'csv' = 'json') =>
    buildUrl(`/api/channels/${channelId}/export/messages?format=${format}`),
  tasksUrl: (channelId: string, format: 'json' | 'csv' = 'json') =>
    buildUrl(`/api/channels/${channelId}/export/tasks?format=${format}`),
}

// Tasks
export const tasks = {
  list: (channelId: string, status?: string) => {
    const qs = status ? `?status=${status}` : ''
    return request<Task[]>(`/api/channels/${channelId}/tasks${qs}`)
  },
  create: (channelId: string, title: string) =>
    request<Task>(`/api/channels/${channelId}/tasks`, { method: 'POST', body: JSON.stringify({ title }) }),
  claim: (channelId: string, taskNumber: number) =>
    request<Task>(`/api/channels/${channelId}/tasks/${taskNumber}/claim`, { method: 'POST' }),
  unclaim: (channelId: string, taskNumber: number) =>
    request<Task>(`/api/channels/${channelId}/tasks/${taskNumber}/unclaim`, { method: 'POST' }),
  updateStatus: (channelId: string, taskNumber: number, status: Task['status']) =>
    request<Task>(`/api/channels/${channelId}/tasks/${taskNumber}/status`, {
      method: 'POST',
      body: JSON.stringify({ status }),
    }),
  getDependencies: (channelId: string, taskNumber: number) =>
    request<{ taskNumber: number; dependsOn: number[] }>(
      `/api/channels/${channelId}/tasks/${taskNumber}/dependencies`
    ),
  executionTimeline: (channelId: string, taskNumber: number) =>
    request<TaskExecutionTimeline>(`/api/channels/${channelId}/tasks/${taskNumber}/execution`),
}

export const zoneTasks = {
  list: (
    zoneId: string,
    params?: {
      status?: string
      channelId?: string
      assignee?: string
      dependency?: string
    },
  ) => {
    const qs = new URLSearchParams()
    if (params?.status) qs.set('status', params.status)
    if (params?.channelId) qs.set('channel', params.channelId)
    if (params?.assignee) qs.set('assignee', params.assignee)
    if (params?.dependency) qs.set('dependency', params.dependency)
    const query = qs.toString()
    return request<Task[]>(`/api/zones/${zoneId}/tasks${query ? `?${query}` : ''}`)
  },
  timeline: (taskId: string) =>
    request<TaskExecutionTimeline>(`/api/tasks/${taskId}/timeline`),
}

// Agent Workspace
export const agentWorkspace = {
  listDir: (agentId: string, path = '/') =>
    request<{ path: string; files: { name: string; isDir: boolean; size: number }[] }>(
      `/api/agents/${agentId}/workspace?path=${encodeURIComponent(path)}`
    ),
  readFile: (agentId: string, path: string) =>
    request<{ content: string; binary: boolean }>(
      `/api/agents/${agentId}/workspace/file?path=${encodeURIComponent(path)}`
    ),
}

// Agent Skills (Phase 3 — flag-gated; creator or zone admin only)
export const agentSkills = {
  list: (agentId: string) =>
    request<{ skills: SkillView[] }>(`/api/agents/${agentId}/skills`),

  install: (agentId: string, libraryId: string) =>
    request<{ installId: string; installPath: string }>(
      `/api/agents/${agentId}/skills`,
      { method: 'POST', body: JSON.stringify({ libraryId }) }
    ),

  uninstall: (agentId: string, installId: string) =>
    request<{ ok: boolean }>(
      `/api/agents/${agentId}/skills/${installId}`,
      { method: 'DELETE' }
    ),

  listFiles: (agentId: string, installId: string) =>
    request<{ installPath: string; files: SkillFileEntry[] }>(
      `/api/agents/${agentId}/skills/${installId}/files`
    ),

  getFile: (agentId: string, installId: string, relPath: string) =>
    request<{ content: string; binary: boolean }>(
      `/api/agents/${agentId}/skills/${installId}/files/${encodeURIComponent(relPath)}`
    ),
}

// Runtime compatibility (Phase 3 — flag-gated)
export const runtimes = {
  compatibility: () =>
    request<Record<string, 'supported' | 'uncertain' | 'unsupported' | 'unknown'>>(
      `/api/runtimes/compatibility`
    ),
}

// Unified Memory (L1/L2 read API — Tasks 6.1/6.2)
export interface MemoryIndex { body: string; version: number }
export interface MemoryTopic { body: string; version: number }

export const memory = {
  getAgentIndex: (agentId: string) =>
    request<MemoryIndex>(`/api/agents/${agentId}/memory/index`),
  getAgentTopic: (agentId: string, type: string, topic: string) =>
    request<MemoryTopic>(`/api/agents/${agentId}/memory/topic?type=${encodeURIComponent(type)}&topic=${encodeURIComponent(topic)}`),
  getChannelIndex: (channelId: string) =>
    request<MemoryIndex>(`/api/channels/${channelId}/memory/index`),
  getChannelTopic: (channelId: string, type: string, topic: string) =>
    request<MemoryTopic>(`/api/channels/${channelId}/memory/topic?type=${encodeURIComponent(type)}&topic=${encodeURIComponent(topic)}`),
}

export const agentSessions = {
  list: (agentId: string, limit = 20, type?: string) =>
    request<AgentSession[]>(
        `/api/agents/${agentId}/sessions?limit=${limit}${type ? `&type=${type}` : ''}`
    ),
  current: (agentId: string) =>
    request<AgentSession | null>(`/api/agents/${agentId}/sessions/current`),
}

// Agent Activity
export const agentActivity = {
  list: (agentId: string, limit = 50, offset = 0) =>
    request<AgentActivityEntry[]>(
      `/api/agents/${agentId}/activity?limit=${limit}&offset=${offset}`
    ),
}

export const overflowStats = {
  list: () => request<OverflowStatsEntry[]>('/api/overflow-stats'),
}

// Bookmarks
export const bookmarks = {
  list: () => request<{ bookmarks: BookmarkEntry[] }>('/api/bookmarks'),
  create: (messageId: string) =>
    request<{ ok: boolean }>(`/api/messages/${messageId}/bookmark`, { method: 'POST' }),
  remove: (messageId: string) =>
    request<{ ok: boolean }>(`/api/messages/${messageId}/bookmark`, { method: 'DELETE' }),
}

// Agent Turns
export const agentTurns = {
  list: (agentId: string, sessionId?: string, limit = 50, offset = 0) => {
    const qs = new URLSearchParams()
    if (sessionId) qs.set('sessionId', sessionId)
    qs.set('limit', String(limit))
    qs.set('offset', String(offset))
    return request<Turn[]>(`/api/agents/${agentId}/turns?${qs}`)
  },
  get: (agentId: string, turnId: string) =>
    request<Turn>(`/api/agents/${agentId}/turns/${turnId}`),
  listBySession: (agentId: string, sessionId: string, limit = 100, offset = 0) => {
    const qs = new URLSearchParams()
    qs.set('limit', String(limit))
    qs.set('offset', String(offset))
    return request<Turn[]>(`/api/agents/${agentId}/sessions/${sessionId}/turns?${qs.toString()}`)
  },
}

// Auth
export const auth = {
  login: (username: string, password: string) =>
    request<{ apiKey: string; user: User }>('/api/auth/login', {
      method: 'POST',
      body: JSON.stringify({ username, password }),
    }),
  signup: (code: string, username: string, email: string, password: string) =>
    request<{ apiKey: string; user: User }>('/api/auth/signup', {
      method: 'POST',
      body: JSON.stringify({ code, username, email, password }),
    }),
  checkInvite: (code: string) =>
    request<{ valid: boolean; expiresAt?: string; remainingUses?: number }>(`/api/auth/invite/${code}`),
  changePassword: (currentPassword: string, newPassword: string) =>
    request<{ ok: boolean }>('/api/users/me/password', {
      method: 'PUT',
      body: JSON.stringify({ currentPassword, newPassword }),
    }),
}

// Invites
export const invites = {
  create: (maxUses?: number, expiresIn?: string) =>
    request<Invite>('/api/invites', {
      method: 'POST',
      body: JSON.stringify({ maxUses, expiresIn }),
    }),
  list: () => request<{ invites: Invite[] }>('/api/invites'),
  revoke: (id: string) => request<{ ok: boolean }>(`/api/invites/${id}`, { method: 'DELETE' }),
}

export const zoneInvites = {
  list: (zoneId: string) => request<{ invites: ZoneInvite[] }>(`/api/zones/${zoneId}/invites`),
  create: (
    zoneId: string,
    payload: {
      invitedUsername?: string
      expiresAt?: string
      maxUses?: number
    } = {},
  ) =>
    request<ZoneInvite>(`/api/zones/${zoneId}/invites`, {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  revoke: (zoneId: string, inviteId: string) =>
    request<{ ok: boolean }>(`/api/zones/${zoneId}/invites/${inviteId}/revoke`, {
      method: 'POST',
    }),
}

// Skill Library (zone-scoped; Phase 2 — gated by skills_v2 flag server-side)
export const zoneSkillLibrary = {
  list: (zoneId: string) =>
    request<{ entries: SkillLibraryEntry[] }>(
      `/api/zones/${zoneId}/skills/library`
    ),
  get: (zoneId: string, libraryId: string) =>
    request<{ entry: SkillLibraryEntry; files: SkillLibraryFileMeta[] }>(
      `/api/zones/${zoneId}/skills/library/${libraryId}`
    ),
  getFile: (zoneId: string, libraryId: string, relPath: string) =>
    request<{ content: string; binary: boolean; size: number }>(
      `/api/zones/${zoneId}/skills/library/${libraryId}/files/${encodeURIComponent(relPath)}`
    ),
  import: (
    zoneId: string,
    body: { url: string; subPath?: string; name?: string },
    opts?: { signal?: AbortSignal },
  ) =>
    request<SkillLibraryImportResponse>(
      `/api/zones/${zoneId}/skills/library`,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
        signal: opts?.signal,
      },
    ),
  reinstall: (zoneId: string, libraryId: string) =>
    request<SkillLibraryReinstallResponse>(
      `/api/zones/${zoneId}/skills/library/${libraryId}/reinstall`,
      { method: 'POST' },
    ),
  remove: (zoneId: string, libraryId: string) =>
    request<{ deleted: string }>(
      `/api/zones/${zoneId}/skills/library/${libraryId}`,
      { method: 'DELETE' },
    ),
}

// Push tokens (mobile-only consumer)
export const pushTokens = {
  register: (input: { platform: 'ios' | 'android'; token: string; deviceId: string; appVersion?: string }) =>
    request<{
      id: string
      userId: string
      platform: 'ios' | 'android'
      token: string
      deviceId: string
      appVersion: string
      createdAt: string
      lastSeenAt: string
      failureCount: number
    }>('/api/push/tokens', {
      method: 'POST',
      body: JSON.stringify(input),
    }),
  unregister: (token: string) =>
    request<{ ok: true }>(`/api/push/tokens/${encodeURIComponent(token)}`, {
      method: 'DELETE',
    }),
}

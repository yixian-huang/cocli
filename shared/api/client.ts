import type {
  Agent,
  AgentActivityEntry,
  AgentSession,
  BookmarkEntry,
  Channel,
  ChannelResponderModeState,
  ChannelResponderPolicy,
  HistoryQuery,
  HistoryResult,
  Message,
  OverflowStatsEntry,
  Plugin,
  PluginCapability,
  PluginRegistration,
  ResponderMode,
  ResponderRole,
  Task,
  TaskExecutionTimeline,
  ThreadSummary,
  Turn,
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
  localStorage.setItem(storageKey('token'), key)
}

export function getApiKey(): string {
  if (!apiKey) {
    apiKey = localStorage.getItem(storageKey('token')) || ''
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
  const requestId = crypto.randomUUID()
  if (import.meta.env.VITE_USE_MOCK === 'true') {
    const { mockHandler } = await import('./mock')
    return mockHandler<T>(path, options)
  }
  const maxRetries = 2
  const retryDelays = [1000, 2000]
  const sentKey = getApiKey()

  bumpInflight(1)
  try {
    for (let attempt = 0; attempt <= maxRetries; attempt++) {
      try {
        const res = await fetch(buildUrl(path), {
          ...options,
          headers: {
            'Content-Type': 'application/json',
            'X-Cocli-Token': sentKey,
            'X-Request-Id': requestId,
            ...options.headers,
          },
        })
        if (res.status === 401 && sentKey) {
          onUnauthorized?.(requestId)
        }
        if (!res.ok) {
          if (res.status >= 500 && attempt < maxRetries) {
            await sleep(retryDelays[attempt] ?? 1000)
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
          await sleep(retryDelays[attempt] ?? 1000)
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

// Channels
export const channels = {
  list: (opts?: { includeArchived?: boolean }) => {
    const q = opts?.includeArchived ? '?includeArchived=true' : ''
    return request<Channel[]>(`/api/channels${q}`)
  },
  create: (name: string, description?: string) =>
    request<Channel>(`/api/channels`, {
      method: 'POST',
      body: JSON.stringify({ name, description }),
    }),
  get: (id: string) => request<Channel>(`/api/channels/${id}`),
  update: (id: string, data: { displayName?: string; description?: string }) =>
    request<Channel>(`/api/channels/${id}`, { method: 'PATCH', body: JSON.stringify(data) }),
  delete: (id: string) => request<void>(`/api/channels/${id}`, { method: 'DELETE' }),
  getMembers: (id: string) =>
    request<{ id: string; memberId: string; memberType: string }[]>(
      `/api/channels/${id}/members`
    ),
  addMember: (id: string, memberId: string, memberType: string) =>
    request<void>(`/api/channels/${id}/members`, {
      method: 'POST',
      body: JSON.stringify({ memberId, memberType }),
    }),
  removeMember: (id: string, memberId: string, memberType: string) =>
    request<void>(`/api/channels/${id}/members`, {
      method: 'DELETE',
      body: JSON.stringify({ memberId, memberType }),
    }),
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
  archive: (id: string, archived: boolean) =>
    request<{ ok: true; archived: boolean }>(`/api/channels/${id}/archive`, {
      method: 'PATCH',
      body: JSON.stringify({ archived }),
    }),
}

// Settings (spec §4.1 — replaces SaaS user-prefs)
export const settings = {
  get: () => request<Record<string, unknown>>(`/api/settings`),
  patch: (payload: Record<string, unknown>) =>
    request<{ ok: true }>(`/api/settings`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    }),
}

// Version + health (spec §4.1)
export const version = {
  get: () => request<{ version: string; commit: string; buildTime?: string }>(`/api/version`),
}

export const health = {
  get: () => request<void>(`/api/health`),
}

// Plugins (spec §4.1 + §4.4)
export const plugins = {
  list: () => request<Plugin[]>(`/api/plugins`),
  register: (name: string, capabilities: PluginCapability[]) =>
    request<PluginRegistration>(`/api/plugins`, {
      method: 'POST',
      body: JSON.stringify({ name, capabilities }),
    }),
  revoke: (id: string) => request<void>(`/api/plugins/${id}`, { method: 'DELETE' }),
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
  messages: (q: string, limit?: number, options?: { signal?: AbortSignal }) => {
    const qs = new URLSearchParams({ q })
    if (limit) qs.set('limit', String(limit))
    return request<{ messages: Message[] }>(`/api/messages/search?${qs}`, {
      signal: options?.signal,
    })
  },
}

// History
export const history = {
  list: (params: HistoryQuery = {}) => {
    const qs = new URLSearchParams()
    if (params.channelId) qs.set('channelId', params.channelId)
    if (params.q) qs.set('q', params.q)
    if (params.from) qs.set('from', params.from)
    if (params.to) qs.set('to', params.to)
    if (params.senderType) qs.set('senderType', params.senderType)
    if (params.senderId) qs.set('senderId', params.senderId)
    qs.set('page', String(params.page ?? 1))
    qs.set('pageSize', String(params.pageSize ?? 30))
    return request<HistoryResult>(`/api/history?${qs.toString()}`)
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
  list: () => request<Channel[]>(`/api/dm`),
  createOrGet: (peerName: string, peerType?: string) =>
    request<Channel>(`/api/dm`, {
      method: 'POST',
      body: JSON.stringify({ peerName, peerType }),
    }),
}

// Agents
export const agents = {
  list: () => request<Agent[]>(`/api/agents`),
  create: (data: {
    name: string
    runtime?: string
    model?: string
    description?: string
    workingRuntime?: string
    workingModel?: string
    chatOnly?: boolean
  }) => request<Agent>(`/api/agents`, { method: 'POST', body: JSON.stringify(data) }),
  get: (id: string) => request<Agent>(`/api/agents/${id}`),
  update: (id: string, data: Partial<Agent>) =>
    request<Agent>(`/api/agents/${id}`, { method: 'PATCH', body: JSON.stringify(data) }),
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
  runtimes: () => request<string[]>(`/api/agents/runtimes`),
}

// Attachments
export const attachments = {
  upload: async (file: File): Promise<{ id: string; filename: string; url: string }> => {
    if (import.meta.env.VITE_USE_MOCK === 'true') {
      return { id: 'mock', filename: file.name, url: 'data:,' }
    }
    const form = new FormData()
    form.append('file', file)
    const requestId = crypto.randomUUID()
    const sentKey = getApiKey()
    bumpInflight(1)
    try {
      const res = await fetch(buildUrl('/api/attachments/upload'), {
        method: 'POST',
        headers: {
          'X-Cocli-Token': sentKey,
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
  listAll: () => request<{ threads: ThreadSummary[] }>(`/api/threads`),
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


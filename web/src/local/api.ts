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
}

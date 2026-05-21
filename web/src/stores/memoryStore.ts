import { create } from 'zustand'
import { memory } from '@/api/client'
import type { MemoryTopic } from '@/api/client'

type IndexKey = `agent:${string}` | `channel:${string}`
type TopicKey = `agent:${string}:${string}:${string}` | `channel:${string}:${string}:${string}`

interface MemoryState {
  entries: Record<IndexKey, string>
  topics: Record<TopicKey, MemoryTopic>
  loadAgentIndex:   (agentId: string) => Promise<void>
  loadChannelIndex: (channelId: string) => Promise<void>
  loadAgentTopic:   (agentId: string, type: string, topic: string) => Promise<void>
  loadChannelTopic: (channelId: string, type: string, topic: string) => Promise<void>
  invalidate:       (scope: 'agent' | 'channel', id: string) => void
}

export const useMemoryStore = create<MemoryState>((set) => ({
  entries: {} as Record<IndexKey, string>,
  topics:  {} as Record<TopicKey, MemoryTopic>,

  loadAgentIndex: async (agentId) => {
    const r = await memory.getAgentIndex(agentId)
    set((s) => ({ entries: { ...s.entries, [`agent:${agentId}`]: r.body } }))
  },

  loadChannelIndex: async (channelId) => {
    const r = await memory.getChannelIndex(channelId)
    set((s) => ({ entries: { ...s.entries, [`channel:${channelId}`]: r.body } }))
  },

  loadAgentTopic: async (agentId, type, topic) => {
    const r = await memory.getAgentTopic(agentId, type, topic)
    set((s) => ({
      topics: { ...s.topics, [`agent:${agentId}:${type}:${topic}`]: r },
    }))
  },

  loadChannelTopic: async (channelId, type, topic) => {
    const r = await memory.getChannelTopic(channelId, type, topic)
    set((s) => ({
      topics: { ...s.topics, [`channel:${channelId}:${type}:${topic}`]: r },
    }))
  },

  invalidate: (scope, id) =>
    set((s) => {
      const idxKey = `${scope}:${id}` as IndexKey
      const topicPrefix = `${scope}:${id}:`

      const newEntries: Record<IndexKey, string> = {}
      for (const k of Object.keys(s.entries) as IndexKey[]) {
        if (k !== idxKey) newEntries[k] = s.entries[k]
      }

      const newTopics: Record<TopicKey, MemoryTopic> = {}
      for (const k of Object.keys(s.topics) as TopicKey[]) {
        if (!k.startsWith(topicPrefix)) newTopics[k] = s.topics[k]
      }

      return { entries: newEntries, topics: newTopics }
    }),
}))

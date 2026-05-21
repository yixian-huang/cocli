import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { useMemoryStore } from './memoryStore'
import type { MemoryTopic } from '@/api/client'

beforeEach(() => {
  useMemoryStore.setState({ entries: {}, topics: {} })
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('memoryStore', () => {
  it('loadAgentIndex fetches and caches by agentId', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue({
      ok: true, status: 200,
      json: async () => ({ body: '- [user_alice](user_alice.md) — Alice notes\n', version: 1 }),
    } as unknown as Response)
    await useMemoryStore.getState().loadAgentIndex('agent-1')
    expect(useMemoryStore.getState().entries['agent:agent-1']).toContain('user_alice')
  })

  it('loadAgentTopic stores body keyed by (agent, type, topic)', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue({
      ok: true, status: 200,
      json: async () => ({ body: 'Alice details', version: 3 }),
    } as unknown as Response)
    await useMemoryStore.getState().loadAgentTopic('agent-1', 'user', 'alice')
    expect(useMemoryStore.getState().topics['agent:agent-1:user:alice']).toEqual({
      body: 'Alice details', version: 3,
    })
  })

  it('loadChannelIndex caches under channel: prefix', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue({
      ok: true, status: 200,
      json: async () => ({ body: '- [project_apollo](project_apollo.md) — Plan\n', version: 5 }),
    } as unknown as Response)
    await useMemoryStore.getState().loadChannelIndex('chan-1')
    expect(useMemoryStore.getState().entries['channel:chan-1']).toContain('project_apollo')
  })

  it('invalidate drops cached entries for a scope/id', async () => {
    useMemoryStore.setState({
      entries: { 'agent:a1': 'idx', 'agent:a2': 'idx2', 'channel:c1': 'cidx' } as unknown as Record<string, string>,
      topics: {
        'agent:a1:user:alice': { body: 'A', version: 1 },
        'agent:a2:user:bob':   { body: 'B', version: 1 },
      } as unknown as Record<string, MemoryTopic>,
    })
    useMemoryStore.getState().invalidate('agent', 'a1')
    const s = useMemoryStore.getState()
    expect(s.entries['agent:a1']).toBeUndefined()
    expect(s.entries['agent:a2']).toBe('idx2')
    expect(s.entries['channel:c1']).toBe('cidx')
    expect(s.topics['agent:a1:user:alice']).toBeUndefined()
    expect(s.topics['agent:a2:user:bob']).toEqual({ body: 'B', version: 1 })
  })
})

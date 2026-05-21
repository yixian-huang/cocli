import { describe, expect, it } from 'vitest'
import { mockHandler } from './mock'

describe('mockHandler', () => {
  it('returns a single hardcoded general channel for GET /api/channels', async () => {
    const result = await mockHandler<{ id: string; name: string }[]>('/api/channels', {})
    expect(result).toHaveLength(1)
    expect(result[0]).toMatchObject({ id: 'general', name: 'general' })
  })

  it('returns version stub for GET /api/version', async () => {
    const result = await mockHandler<{ version: string }>('/api/version', {})
    expect(result.version).toContain('mock')
  })

  it('returns undefined (204) for GET /api/health', async () => {
    const result = await mockHandler<void>('/api/health', {})
    expect(result).toBeUndefined()
  })

  it('returns empty array for unmocked GET paths', async () => {
    const result = await mockHandler<unknown[]>('/api/agents', {})
    expect(result).toEqual([])
  })

  it('returns undefined for unmocked POST/PATCH/DELETE paths', async () => {
    const result = await mockHandler<void>('/api/foo', { method: 'POST' })
    expect(result).toBeUndefined()
  })

  it('matches /api/channels even with query string', async () => {
    const result = await mockHandler<{ id: string }[]>(
      '/api/channels?includeArchived=true', {}
    )
    expect(result).toHaveLength(1)
    expect(result[0]?.id).toBe('general')
  })
})

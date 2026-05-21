import { describe, expect, it, beforeEach } from 'vitest'
import { usePluginsStore } from './pluginsStore'
import { storageKey } from '@shared/brand'

const KEY = storageKey('cocli-plugins')

beforeEach(() => {
  localStorage.clear()
  usePluginsStore.setState({ plugins: [] })
})

describe('usePluginsStore', () => {
  it('starts with empty plugins', () => {
    expect(usePluginsStore.getState().plugins).toEqual([])
  })

  it('list() hydrates from localStorage', async () => {
    const stored = [{
      id: 'p1', name: 'telegram-bot',
      capabilities: ['inbound-bridge'], createdAt: '2026-05-21T00:00:00Z',
      lastSeenAt: null,
    }]
    localStorage.setItem(KEY, JSON.stringify(stored))
    await usePluginsStore.getState().list()
    expect(usePluginsStore.getState().plugins).toEqual(stored)
  })

  it('register() returns plugin + token, persists, and stores plugin', async () => {
    const { plugin, token } = await usePluginsStore.getState().register(
      'telegram-bot',
      ['inbound-bridge', 'outbound-bridge'],
    )
    expect(plugin.id).toMatch(/^[0-9a-f-]{36}$/)
    expect(plugin.name).toBe('telegram-bot')
    expect(plugin.capabilities).toEqual(['inbound-bridge', 'outbound-bridge'])
    expect(plugin.lastSeenAt).toBeNull()
    expect(token).toMatch(/^[0-9a-f-]{36}$/)
    expect(usePluginsStore.getState().plugins).toHaveLength(1)
    expect(JSON.parse(localStorage.getItem(KEY)!)).toHaveLength(1)
  })

  it('revoke() removes plugin and persists', async () => {
    const { plugin } = await usePluginsStore.getState().register('a', ['inbound-bridge'])
    await usePluginsStore.getState().revoke(plugin.id)
    expect(usePluginsStore.getState().plugins).toHaveLength(0)
    expect(JSON.parse(localStorage.getItem(KEY)!)).toHaveLength(0)
  })

  it('token is NOT included in persisted localStorage payload', async () => {
    const { token } = await usePluginsStore.getState().register('a', ['inbound-bridge'])
    const raw = localStorage.getItem(KEY)!
    expect(raw).not.toContain(token)
  })
})

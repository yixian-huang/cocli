import { describe, expect, it } from 'vitest'
import { useUserStore } from './userStore'

describe('useUserStore (single-tenant local shim)', () => {
  it('returns the hardcoded owner user on first read', () => {
    const { user } = useUserStore.getState()
    expect(user).not.toBeNull()
    expect(user?.id).toBe('local')
    expect(user?.name).toBe('owner')
    expect(user?.displayName).toBe('owner')
  })

  it('init() is a no-op', () => {
    const before = useUserStore.getState().user
    useUserStore.getState().init()
    expect(useUserStore.getState().user).toBe(before)
  })

  it('exposes a loading=false synchronously', () => {
    expect(useUserStore.getState().loading).toBe(false)
  })
})

import { beforeEach, describe, expect, it } from 'vitest'
import { storageKey } from '@/brand'
import { useSidebarPrefsStore } from './sidebarPrefsStore'

const hiddenDMStorageKey = (zoneId: string) => storageKey(`hidden-dms:${zoneId}`)

describe('sidebarPrefsStore', () => {
  beforeEach(() => {
    localStorage.clear()
    useSidebarPrefsStore.setState({
      zoneId: null,
      hiddenDMIds: new Set(),
    })
  })

  it('keeps hidden DMs isolated by zone', () => {
    useSidebarPrefsStore.getState().setZone('z1')
    useSidebarPrefsStore.getState().hideDM('dm1')

    useSidebarPrefsStore.getState().setZone('z2')
    expect(useSidebarPrefsStore.getState().isDMHidden('dm1')).toBe(false)

    useSidebarPrefsStore.getState().setZone('z1')
    expect(useSidebarPrefsStore.getState().isDMHidden('dm1')).toBe(true)
  })

  it('hides and unhides DMs in the active zone', () => {
    useSidebarPrefsStore.getState().setZone('z1')

    useSidebarPrefsStore.getState().hideDM('dm1')
    expect(useSidebarPrefsStore.getState().isDMHidden('dm1')).toBe(true)
    expect(JSON.parse(localStorage.getItem(hiddenDMStorageKey('z1')) ?? '[]')).toEqual(['dm1'])

    useSidebarPrefsStore.getState().unhideDM('dm1')
    expect(useSidebarPrefsStore.getState().isDMHidden('dm1')).toBe(false)
    expect(JSON.parse(localStorage.getItem(hiddenDMStorageKey('z1')) ?? '[]')).toEqual([])
  })

  it('reads invalid localStorage values as empty hidden DMs', () => {
    localStorage.setItem(hiddenDMStorageKey('bad-json'), '{')
    localStorage.setItem(hiddenDMStorageKey('not-array'), JSON.stringify({ dm1: true }))
    localStorage.setItem(hiddenDMStorageKey('mixed'), JSON.stringify(['dm1', 1, null, 'dm2']))

    expect(() => useSidebarPrefsStore.getState().setZone('bad-json')).not.toThrow()
    expect(useSidebarPrefsStore.getState().isDMHidden('dm1')).toBe(false)

    expect(() => useSidebarPrefsStore.getState().setZone('not-array')).not.toThrow()
    expect(useSidebarPrefsStore.getState().isDMHidden('dm1')).toBe(false)

    expect(() => useSidebarPrefsStore.getState().setZone('mixed')).not.toThrow()
    expect(useSidebarPrefsStore.getState().isDMHidden('dm1')).toBe(true)
    expect(useSidebarPrefsStore.getState().isDMHidden('dm2')).toBe(true)
  })
})

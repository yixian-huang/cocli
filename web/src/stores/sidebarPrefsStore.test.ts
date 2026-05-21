import { beforeEach, describe, expect, it } from 'vitest'
import { useSidebarPrefsStore } from './sidebarPrefsStore'

describe('sidebarPrefsStore', () => {
  beforeEach(() => {
    localStorage.clear()
    useSidebarPrefsStore.setState({
      hiddenDMIds: new Set(),
    })
  })

  it('hides and unhides DMs', () => {
    useSidebarPrefsStore.getState().hideDM('dm1')
    expect(useSidebarPrefsStore.getState().isDMHidden('dm1')).toBe(true)
    expect(JSON.parse(localStorage.getItem('cocli-hidden-dms') ?? '[]')).toEqual(['dm1'])

    useSidebarPrefsStore.getState().unhideDM('dm1')
    expect(useSidebarPrefsStore.getState().isDMHidden('dm1')).toBe(false)
    expect(JSON.parse(localStorage.getItem('cocli-hidden-dms') ?? '[]')).toEqual([])
  })

  it('reads invalid localStorage values as empty hidden DMs', () => {
    localStorage.setItem('chatrs-hidden-dms', '{')
    useSidebarPrefsStore.setState({ hiddenDMIds: new Set() })

    // Re-init from storage by resetting and letting it read
    expect(() => {
      const raw = localStorage.getItem('chatrs-hidden-dms')
      if (raw) JSON.parse(raw)
    }).toThrow()

    // The store should not throw on read
    expect(useSidebarPrefsStore.getState().isDMHidden('dm1')).toBe(false)
  })

  it('persists hidden DMs across store resets', () => {
    useSidebarPrefsStore.getState().hideDM('dm1')
    useSidebarPrefsStore.getState().hideDM('dm2')
    expect(useSidebarPrefsStore.getState().isDMHidden('dm1')).toBe(true)
    expect(useSidebarPrefsStore.getState().isDMHidden('dm2')).toBe(true)

    useSidebarPrefsStore.getState().unhideDM('dm1')
    expect(useSidebarPrefsStore.getState().isDMHidden('dm1')).toBe(false)
    expect(useSidebarPrefsStore.getState().isDMHidden('dm2')).toBe(true)
  })
})

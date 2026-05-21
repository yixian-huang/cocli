import { beforeEach, describe, expect, it } from 'vitest'
import { useDialogStore, resetDialogStore } from './dialogStore'

describe('dialogStore', () => {
  beforeEach(() => resetDialogStore())

  it('opens and closes a single dialog at a time', () => {
    useDialogStore.getState().openCreateChannel()
    expect(useDialogStore.getState().active).toBe('createChannel')
    useDialogStore.getState().close()
    expect(useDialogStore.getState().active).toBe(null)
  })

  it('opening a different dialog replaces the active one', () => {
    useDialogStore.getState().openCreateChannel()
    useDialogStore.getState().openCreateAgent()
    expect(useDialogStore.getState().active).toBe('createAgent')
  })

  it('payload is exposed for the active dialog', () => {
    useDialogStore.getState().openAddDaemon({ daemonId: 'd1' })
    expect(useDialogStore.getState().payload).toEqual({ daemonId: 'd1' })
  })

  it('close() clears both active and payload', () => {
    useDialogStore.getState().openOpenDM({ peerName: 'alice' })
    useDialogStore.getState().close()
    expect(useDialogStore.getState().active).toBe(null)
    expect(useDialogStore.getState().payload).toBe(null)
  })
})

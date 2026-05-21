import { describe, it, expect } from 'vitest'
import { useWSStore } from './wsStore'

describe('wsStore', () => {
  it('starts with connecting status', () => {
    expect(useWSStore.getState().status).toBe('connecting')
  })

  it('setStatus updates the status', () => {
    useWSStore.getState().setStatus('connected')
    expect(useWSStore.getState().status).toBe('connected')

    useWSStore.getState().setStatus('disconnected')
    expect(useWSStore.getState().status).toBe('disconnected')
  })
})

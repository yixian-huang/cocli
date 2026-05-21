import { describe, it, expect, beforeEach } from 'vitest'
import { useThreadInboxStore } from './threadInboxStore'

const makeThread = (id: string, done = false, lastActivityAt = '2026-04-07T00:00:00Z') => ({
  id,
  parentMessage: {
    id: `msg-${id}`,
    channelId: `ch-${id}`,
    senderType: 'user' as const,
    senderName: 'alice',
    content: `Thread message ${id}`,
    seq: 1,
    createdAt: '2026-04-07T00:00:00Z',
  },
  parentChannelName: 'general',
  replyCount: 3,
  lastActivityAt,
  done,
})

describe('threadInboxStore', () => {
  beforeEach(() => {
    useThreadInboxStore.setState({
      threads: [],
      showDone: false,
      loading: false,
    })
  })

  it('starts with empty threads', () => {
    expect(useThreadInboxStore.getState().threads).toEqual([])
  })

  it('setShowDone toggles visibility', () => {
    useThreadInboxStore.getState().setShowDone(true)
    expect(useThreadInboxStore.getState().showDone).toBe(true)
  })

  it('updateThread updates done flag', () => {
    useThreadInboxStore.setState({
      threads: [makeThread('t1'), makeThread('t2')],
    })
    useThreadInboxStore.getState().updateThread('t1', { done: true })
    const t1 = useThreadInboxStore.getState().threads.find((t) => t.id === 't1')
    expect(t1?.done).toBe(true)
  })

  it('updateThread updates lastActivityAt and replyCount', () => {
    useThreadInboxStore.setState({
      threads: [makeThread('t1')],
    })
    useThreadInboxStore.getState().updateThread('t1', {
      lastActivityAt: '2026-04-08T00:00:00Z',
      replyCount: 5,
    })
    const t1 = useThreadInboxStore.getState().threads.find((t) => t.id === 't1')
    expect(t1?.lastActivityAt).toBe('2026-04-08T00:00:00Z')
    expect(t1?.replyCount).toBe(5)
  })

  it('getVisibleThreads filters by done when showDone is false', () => {
    useThreadInboxStore.setState({
      threads: [makeThread('t1', false), makeThread('t2', true)],
      showDone: false,
    })
    const visible = useThreadInboxStore.getState().getVisibleThreads()
    expect(visible).toHaveLength(1)
    expect(visible[0].id).toBe('t1')
  })

  it('getVisibleThreads shows all when showDone is true', () => {
    useThreadInboxStore.setState({
      threads: [makeThread('t1', false), makeThread('t2', true)],
      showDone: true,
    })
    const visible = useThreadInboxStore.getState().getVisibleThreads()
    expect(visible).toHaveLength(2)
  })
})

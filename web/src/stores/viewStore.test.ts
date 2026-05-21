import { describe, it, expect, beforeEach } from 'vitest'
import { useViewStore } from './viewStore'

beforeEach(() => {
  useViewStore.setState({
    activeAgentId: null,
    agentReturnTo: null,
    quotedMessage: null,
    agentSubview: {},
    activeDrawer: null,
    historyDrawerSegment: null,
  })
})

describe('viewStore subview + drawer', () => {
  it('getSubview defaults to "main" for unknown agentId', () => {
    expect(useViewStore.getState().getSubview('agent-x')).toBe('main')
  })

  it('setAgentSubview persists per agent', () => {
    useViewStore.getState().setAgentSubview('a', 'settings')
    useViewStore.getState().setAgentSubview('b', 'main')
    expect(useViewStore.getState().getSubview('a')).toBe('settings')
    expect(useViewStore.getState().getSubview('b')).toBe('main')
  })

  it('switching agent closes any open drawer', () => {
    useViewStore.getState().setActiveAgent('a')
    useViewStore.getState().setActiveDrawer('live')
    expect(useViewStore.getState().activeDrawer).toBe('live')

    useViewStore.getState().setActiveAgent('b')
    expect(useViewStore.getState().activeDrawer).toBeNull()
  })

  it('entering settings closes any open drawer', () => {
    useViewStore.getState().setActiveAgent('a')
    useViewStore.getState().setActiveDrawer('history')
    useViewStore.getState().setAgentSubview('a', 'settings')
    expect(useViewStore.getState().activeDrawer).toBeNull()
  })

  it('toggling the same drawer key twice closes it (via setActiveDrawer with same key)', () => {
    useViewStore.getState().setActiveDrawer('memory')
    expect(useViewStore.getState().activeDrawer).toBe('memory')
    useViewStore.getState().toggleDrawer('memory')
    expect(useViewStore.getState().activeDrawer).toBeNull()
    useViewStore.getState().toggleDrawer('memory')
    expect(useViewStore.getState().activeDrawer).toBe('memory')
  })

  it('clearActiveAgent also clears drawer', () => {
    useViewStore.getState().setActiveAgent('a')
    useViewStore.getState().setActiveDrawer('live')
    useViewStore.getState().clearActiveAgent()
    expect(useViewStore.getState().activeAgentId).toBeNull()
    expect(useViewStore.getState().activeDrawer).toBeNull()
  })

  it('openHistoryAt opens history drawer with the given segment', () => {
    useViewStore.getState().openHistoryAt('activity')
    expect(useViewStore.getState().activeDrawer).toBe('history')
    expect(useViewStore.getState().historyDrawerSegment).toBe('activity')
  })

  it('switching agent clears historyDrawerSegment too', () => {
    useViewStore.getState().setActiveAgent('a')
    useViewStore.getState().openHistoryAt('activity')
    useViewStore.getState().setActiveAgent('b')
    expect(useViewStore.getState().historyDrawerSegment).toBeNull()
  })

  it('subview persists when switching agents (spec §3.1)', () => {
    useViewStore.getState().setActiveAgent('a')
    useViewStore.getState().setAgentSubview('a', 'settings')
    useViewStore.getState().setActiveAgent('b')
    expect(useViewStore.getState().getSubview('a')).toBe('settings')
    expect(useViewStore.getState().getSubview('b')).toBe('main')
    useViewStore.getState().setActiveAgent('a')
    expect(useViewStore.getState().getSubview('a')).toBe('settings')
  })
})

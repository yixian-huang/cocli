import { describe, expect, it, beforeEach, vi } from 'vitest'
import { useWizardStore } from './wizardStore'
import { useAgentStore } from './agentStore'
import { storageKey } from '@shared/brand'

const KEY_COMPLETE = storageKey('first-run-complete')
const KEY_STATE = storageKey('wizard-state')

beforeEach(() => {
  localStorage.clear()
  useWizardStore.setState({
    step: 1,
    complete: false,
    claudePath: '',
    detectedAt: null,
    draftAgent: { name: '', model: 'claude-sonnet-4-6' },
  })
  useAgentStore.setState({ agents: [] })
})

describe('useWizardStore', () => {
  it('starts at step 1 with empty state', () => {
    const s = useWizardStore.getState()
    expect(s.step).toBe(1)
    expect(s.complete).toBe(false)
    expect(s.claudePath).toBe('')
    expect(s.draftAgent).toEqual({ name: '', model: 'claude-sonnet-4-6' })
  })

  it('next() advances step and caps at 3', () => {
    useWizardStore.getState().next()
    expect(useWizardStore.getState().step).toBe(2)
    useWizardStore.getState().next()
    expect(useWizardStore.getState().step).toBe(3)
    useWizardStore.getState().next()
    expect(useWizardStore.getState().step).toBe(3)
  })

  it('back() retreats and caps at 1', () => {
    useWizardStore.setState({ step: 3 })
    useWizardStore.getState().back()
    expect(useWizardStore.getState().step).toBe(2)
    useWizardStore.getState().back()
    expect(useWizardStore.getState().step).toBe(1)
    useWizardStore.getState().back()
    expect(useWizardStore.getState().step).toBe(1)
  })

  it('setClaudePath() updates path', () => {
    useWizardStore.getState().setClaudePath('/usr/bin/claude')
    expect(useWizardStore.getState().claudePath).toBe('/usr/bin/claude')
  })

  it('detectClaudePath() sets detectedAt after a tick', async () => {
    vi.useFakeTimers()
    const p = useWizardStore.getState().detectClaudePath()
    vi.advanceTimersByTime(700)
    await p
    expect(useWizardStore.getState().detectedAt).toBeTruthy()
    vi.useRealTimers()
  })

  it('setDraftAgent() partial-merges fields', () => {
    useWizardStore.getState().setDraftAgent({ name: '@bot' })
    expect(useWizardStore.getState().draftAgent.name).toBe('@bot')
    expect(useWizardStore.getState().draftAgent.model).toBe('claude-sonnet-4-6')
  })

  it('finish() persists complete flag + pushes draft into agentStore', () => {
    useWizardStore.setState({
      draftAgent: { name: '@assistant', model: 'claude-sonnet-4-6' },
    })
    useWizardStore.getState().finish()
    expect(useWizardStore.getState().complete).toBe(true)
    expect(localStorage.getItem(KEY_COMPLETE)).toBe('true')
    const inserted = useAgentStore.getState().agents.find((a) => a.name === '@assistant')
    expect(inserted).toBeTruthy()
    expect(inserted?.model).toBe('claude-sonnet-4-6')
  })

  it('init() honors prior completion flag', () => {
    localStorage.setItem(KEY_COMPLETE, 'true')
    useWizardStore.getState().init()
    expect(useWizardStore.getState().complete).toBe(true)
  })

  it('init() restores in-progress state', () => {
    localStorage.setItem(
      KEY_STATE,
      JSON.stringify({
        step: 2,
        claudePath: '/x',
        draftAgent: { name: '@a', model: 'claude-haiku-4-5' },
      }),
    )
    useWizardStore.getState().init()
    const s = useWizardStore.getState()
    expect(s.step).toBe(2)
    expect(s.claudePath).toBe('/x')
    expect(s.draftAgent).toEqual({ name: '@a', model: 'claude-haiku-4-5' })
  })

  it('honors ?skip-wizard=1 on init', () => {
    const orig = window.location.search
    Object.defineProperty(window, 'location', {
      value: { ...window.location, search: '?skip-wizard=1' },
      writable: true,
    })
    useWizardStore.getState().init()
    expect(useWizardStore.getState().complete).toBe(true)
    Object.defineProperty(window, 'location', {
      value: { ...window.location, search: orig },
      writable: true,
    })
  })
})

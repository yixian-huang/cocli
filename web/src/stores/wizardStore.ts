import { create } from 'zustand'
import { useAgentStore } from './agentStore'
import { storageKey } from '@shared/brand'
import type { Agent } from '@shared/types'

export type Model = 'claude-sonnet-4-6' | 'claude-haiku-4-5' | 'claude-opus-4-7'

export interface DraftAgent {
  name: string
  model: Model
}

interface WizardState {
  step: 1 | 2 | 3
  complete: boolean
  claudePath: string
  detectedAt: string | null
  draftAgent: DraftAgent
  init: () => void
  next: () => void
  back: () => void
  setClaudePath: (p: string) => void
  detectClaudePath: () => Promise<void>
  setDraftAgent: (a: Partial<DraftAgent>) => void
  finish: () => void
}

const KEY_COMPLETE = 'first-run-complete'
const KEY_STATE = 'wizard-state'

function persistState(state: Pick<WizardState, 'step' | 'claudePath' | 'draftAgent'>) {
  localStorage.setItem(
    storageKey(KEY_STATE),
    JSON.stringify({
      step: state.step,
      claudePath: state.claudePath,
      draftAgent: state.draftAgent,
    }),
  )
}

export const useWizardStore = create<WizardState>((set, get) => ({
  step: 1,
  complete: false,
  claudePath: '',
  detectedAt: null,
  draftAgent: { name: '', model: 'claude-sonnet-4-6' },

  init: () => {
    if (new URLSearchParams(window.location.search).get('skip-wizard') === '1') {
      get().finish()
      return
    }
    if (localStorage.getItem(storageKey(KEY_COMPLETE)) === 'true') {
      set({ complete: true })
      return
    }
    const raw = localStorage.getItem(storageKey(KEY_STATE))
    if (!raw) return
    try {
      const parsed = JSON.parse(raw) as Partial<Pick<WizardState, 'step' | 'claudePath' | 'draftAgent'>>
      set({
        step: (parsed.step as 1 | 2 | 3) ?? 1,
        claudePath: parsed.claudePath ?? '',
        draftAgent: parsed.draftAgent ?? { name: '', model: 'claude-sonnet-4-6' },
      })
    } catch {
      /* corrupt JSON — start fresh */
    }
  },

  next: () => {
    const cur = get().step
    const nxt = (cur < 3 ? cur + 1 : 3) as 1 | 2 | 3
    set({ step: nxt })
    persistState(get())
  },

  back: () => {
    const cur = get().step
    const prv = (cur > 1 ? cur - 1 : 1) as 1 | 2 | 3
    set({ step: prv })
    persistState(get())
  },

  setClaudePath: (p) => {
    set({ claudePath: p })
    persistState(get())
  },

  detectClaudePath: async () => {
    await new Promise((r) => setTimeout(r, 600))
    set({ detectedAt: new Date().toISOString() })
  },

  setDraftAgent: (patch) => {
    set({ draftAgent: { ...get().draftAgent, ...patch } })
    persistState(get())
  },

  finish: () => {
    const draft = get().draftAgent
    if (draft.name) {
      const now = new Date().toISOString()
      const agent: Agent = {
        id: crypto.randomUUID(),
        name: draft.name,
        runtime: 'claude',
        model: draft.model,
        status: 'offline',
        createdAt: now,
        updatedAt: now,
      }
      useAgentStore.setState((s) => ({ agents: [...s.agents, agent] }))
    }
    localStorage.setItem(storageKey(KEY_COMPLETE), 'true')
    localStorage.removeItem(storageKey(KEY_STATE))
    set({ complete: true })
  },
}))

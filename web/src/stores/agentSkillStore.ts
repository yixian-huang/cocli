import { create } from 'zustand'
import { agentSkills, runtimes } from '@/api/client'
import type { SkillView } from '@/lib/types'

type CompatMap = Record<string, 'supported' | 'uncertain' | 'unsupported' | 'unknown'>

interface AgentSkillState {
  skillsByAgent: Record<string, SkillView[]>
  loadingByAgent: Record<string, boolean>
  errorByAgent: Record<string, string | null>
  compatibility: CompatMap | null

  fetchForAgent: (agentId: string) => Promise<void>
  install: (agentId: string, libraryId: string) => Promise<void>
  uninstall: (agentId: string, installId: string) => Promise<void>
  loadCompatibility: () => Promise<void>
}

export const useAgentSkillStore = create<AgentSkillState>((set, get) => ({
  skillsByAgent: {},
  loadingByAgent: {},
  errorByAgent: {},
  compatibility: null,

  async fetchForAgent(agentId) {
    set((s) => ({
      loadingByAgent: { ...s.loadingByAgent, [agentId]: true },
      errorByAgent: { ...s.errorByAgent, [agentId]: null },
    }))
    try {
      const res = await agentSkills.list(agentId)
      set((s) => ({
        skillsByAgent: { ...s.skillsByAgent, [agentId]: res.skills || [] },
        loadingByAgent: { ...s.loadingByAgent, [agentId]: false },
      }))
    } catch (e) {
      set((s) => ({
        loadingByAgent: { ...s.loadingByAgent, [agentId]: false },
        errorByAgent: {
          ...s.errorByAgent,
          [agentId]: e instanceof Error ? e.message : 'Failed to load skills',
        },
      }))
    }
  },

  async install(agentId, libraryId) {
    await agentSkills.install(agentId, libraryId)
    await get().fetchForAgent(agentId)
  },

  async uninstall(agentId, installId) {
    await agentSkills.uninstall(agentId, installId)
    await get().fetchForAgent(agentId)
  },

  async loadCompatibility() {
    if (get().compatibility) return
    const m = await runtimes.compatibility()
    set({ compatibility: m })
  },
}))

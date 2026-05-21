import { create } from 'zustand'
import type { Message } from '@/lib/types'

export type AgentSubview = 'main' | 'settings'
export type DrawerKey = 'live' | 'history' | 'memory'
export type HistorySegment = 'sessions' | 'activity'

interface ViewState {
  activeAgentId: string | null
  /** When set, back from agent view navigates here (e.g. daemon manage deep link). */
  agentReturnTo: string | null
  quotedMessage: Message | null
  agentSubview: Record<string, AgentSubview>
  activeDrawer: DrawerKey | null
  historyDrawerSegment: HistorySegment | null

  setActiveAgent: (id: string, returnTo?: string | null) => void
  clearActiveAgent: () => void
  setQuotedMessage: (message: Message | null) => void

  getSubview: (agentId: string) => AgentSubview
  setAgentSubview: (agentId: string, subview: AgentSubview) => void
  setActiveDrawer: (key: DrawerKey | null) => void
  toggleDrawer: (key: DrawerKey) => void
  openHistoryAt: (segment: HistorySegment) => void
}

export const useViewStore = create<ViewState>((set, get) => ({
  activeAgentId: null,
  agentReturnTo: null,
  quotedMessage: null,
  agentSubview: {},
  activeDrawer: null,
  historyDrawerSegment: null,

  setActiveAgent: (id, returnTo) =>
    set({
      activeAgentId: id,
      agentReturnTo: returnTo ?? null,
      activeDrawer: null,
      historyDrawerSegment: null,
    }),
  clearActiveAgent: () =>
    set({ activeAgentId: null, agentReturnTo: null, activeDrawer: null, historyDrawerSegment: null }),
  setQuotedMessage: (message) => set({ quotedMessage: message }),

  getSubview: (agentId) => get().agentSubview[agentId] ?? 'main',
  setAgentSubview: (agentId, subview) =>
    set((s) => ({
      agentSubview: { ...s.agentSubview, [agentId]: subview },
      activeDrawer: subview === 'settings' ? null : s.activeDrawer,
      historyDrawerSegment: subview === 'settings' ? null : s.historyDrawerSegment,
    })),
  setActiveDrawer: (key) => set({ activeDrawer: key }),
  toggleDrawer: (key) =>
    set((s) => ({ activeDrawer: s.activeDrawer === key ? null : key })),
  openHistoryAt: (segment) => set({ activeDrawer: 'history', historyDrawerSegment: segment }),
}))

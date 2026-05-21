import { create } from 'zustand'
import type { Agent, AgentAttentionState, AgentStatus, PriorityClass, TrajectoryEntry, Turn } from '@/lib/types'
import { agents as agentsApi } from '@/api/client'

type ActivityMetrics = {
  lastInputTokens?: number
  totalOutputTokens?: number
  contextWindow?: number
  totalCostUSD?: number
  turnCount?: number
}

type ActivityAttention = {
  attentionState?: AgentAttentionState
  focusTaskId?: string
  focusScope?: string
  focusSince?: number
  priorityClass?: PriorityClass
  preempted?: boolean
}

const statusSet = new Set<AgentStatus>(['offline', 'online', 'working', 'error'])

function isAgentStatus(value: string): value is AgentStatus {
  return statusSet.has(value as AgentStatus)
}

function attentionForStatus(status: AgentStatus): AgentAttentionState {
  switch (status) {
    case 'working':
      return 'working'
    default:
      return 'idle'
  }
}

interface AgentState {
  agents: Agent[]
  loading: boolean
  fetchAgents: () => Promise<void>
  updateStatus: (agentId: string, status: Agent['status'], errorDetail?: string) => void
  updateActivity: (agentId: string, activity: string, detail?: string, trajectory?: string[], metrics?: ActivityMetrics, attention?: ActivityAttention) => void
  startAgent: (id: string) => Promise<void>
  stopAgent: (id: string, force?: boolean) => Promise<void>
  cancelAgentTurn: (id: string) => Promise<void>
  steerAgentTurn: (id: string, input: string) => Promise<void>
  // Turn-level activity
  turns: Record<string, Turn[]>
  currentTurnEntries: Record<string, TrajectoryEntry[]>
  setTurns: (agentId: string, turns: Turn[]) => void
  appendEntry: (agentId: string, entry: TrajectoryEntry) => void
  finalizeTurn: (agentId: string, turn: Turn) => void
}

export const useAgentStore = create<AgentState>((set) => ({
  agents: [],
  loading: true,

  fetchAgents: async () => {
    try {
      const agents = await agentsApi.list()
      set({ agents: agents || [], loading: false })
    } catch {
      set({ loading: false })
    }
  },

  updateStatus: (agentId, status, errorDetail) =>
    set((s) => ({
      agents: s.agents.map((a) =>
        a.id === agentId
          ? {
              ...a,
              status,
              attentionState:
                (a.attentionState === 'stalled' || a.attentionState === 'context_pressure') &&
                status === 'working'
                  ? a.attentionState
                  : attentionForStatus(status),
              errorDetail,
            }
          : a
      ),
    })),

  updateActivity: (agentId, activity, detail, trajectory, metrics, attention) =>
    set((s) => ({
      agents: s.agents.map((a) =>
        a.id !== agentId
          ? a
          : (() => {
              const nextStatus = isAgentStatus(activity) ? activity : a.status
              const recoveredSignal =
                activity === 'recovered' ||
                detail === 'notification_cap_recovered' ||
                detail === 'notification_cap_reset' ||
                (typeof detail === 'string' && detail.startsWith('context_pressure_recovered'))
              const nextAttentionState =
                attention?.attentionState ??
                (recoveredSignal
                  ? 'working'
                  : isAgentStatus(activity)
                    ? ((a.attentionState === 'stalled' || a.attentionState === 'context_pressure') &&
                       activity === 'working'
                        ? a.attentionState
                        : attentionForStatus(activity))
                    : a.attentionState)
              const inFocusFlow = nextAttentionState === 'focus' || nextAttentionState === 'preempting'
              return {
                ...a,
                status: nextStatus,
                activity,
                detail,
                trajectory,
                attentionState: nextAttentionState,
                focusTaskId: inFocusFlow ? (attention?.focusTaskId ?? a.focusTaskId) : undefined,
                focusScope: inFocusFlow ? (attention?.focusScope ?? a.focusScope) : undefined,
                focusSince: inFocusFlow ? (attention?.focusSince ?? a.focusSince) : undefined,
                priorityClass: attention?.priorityClass ?? a.priorityClass,
                preempted: attention?.preempted ?? (inFocusFlow ? a.preempted : false),
                errorDetail: undefined,
                ...(metrics?.lastInputTokens != null && { lastInputTokens: metrics.lastInputTokens }),
                ...(metrics?.totalOutputTokens != null && { totalOutputTokens: metrics.totalOutputTokens }),
                ...(metrics?.contextWindow != null && { contextWindow: metrics.contextWindow }),
                ...(metrics?.totalCostUSD != null && { totalCostUSD: metrics.totalCostUSD }),
                ...(metrics?.turnCount != null && { turnCount: metrics.turnCount }),
              }
            })()
      ),
    })),

  startAgent: async (id) => {
    await agentsApi.start(id)
  },

  stopAgent: async (id, force) => {
    await agentsApi.stop(id, force)
  },

  cancelAgentTurn: async (id) => {
    await agentsApi.cancelTurn(id)
  },

  steerAgentTurn: async (id, input) => {
    await agentsApi.steerTurn(id, input)
  },

  turns: {},
  currentTurnEntries: {},

  setTurns: (agentId, turns) =>
    set((s) => ({
      turns: {
        ...s.turns,
        [agentId]: turns.map((t) => ({ ...t, entries: t.entries ?? [] })),
      },
    })),

  appendEntry: (agentId, entry) =>
    set((s) => ({
      currentTurnEntries: {
        ...s.currentTurnEntries,
        [agentId]: [...(s.currentTurnEntries[agentId] || []), entry],
      },
    })),

  finalizeTurn: (agentId, turn) =>
    set((s) => ({
      turns: {
        ...s.turns,
        [agentId]: [...(s.turns[agentId] || []), { ...turn, entries: turn.entries ?? [] }],
      },
      currentTurnEntries: {
        ...s.currentTurnEntries,
        [agentId]: [],
      },
    })),
}))

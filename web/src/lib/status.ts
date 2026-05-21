import {
  AlertTriangle,
  Clock,
  Gauge,
  Loader2,
  RefreshCw,
  SlidersHorizontal,
  type LucideIcon,
  Zap,
} from 'lucide-react'
import type { Agent, AgentAttentionState, Task } from '@/lib/types'

export type AgentAttentionTone = 'success' | 'info' | 'warn' | 'danger' | 'neutral'

type AgentAttentionMeta = {
  label: string
  tone: AgentAttentionTone
  Icon?: LucideIcon
  title?: string
  animated?: boolean
}

const ATTENTION_META: Record<AgentAttentionState, AgentAttentionMeta> = {
  idle: {
    label: 'Idle',
    tone: 'neutral',
  },
  working: {
    label: 'Working',
    tone: 'success',
    Icon: Loader2,
    animated: true,
  },
  focus: {
    label: 'Focus',
    tone: 'info',
    Icon: Zap,
    title: 'Agent is locked on the current task',
  },
  preempting: {
    label: 'Preempting',
    tone: 'warn',
    Icon: RefreshCw,
    animated: true,
    title: 'Higher-priority work is interrupting the current flow',
  },
  stalled: {
    label: 'Stalled',
    tone: 'danger',
    Icon: AlertTriangle,
    title: '通知循环被暂停（点击 restart）',
  },
  context_pressure: {
    label: 'Context high',
    tone: 'warn',
    Icon: Gauge,
    title: 'Context usage is approaching the fork threshold',
  },
  context_overflow: {
    label: 'Context overflow',
    tone: 'danger',
    Icon: AlertTriangle,
    title: 'Context window overflow was detected',
  },
  backstop_threshold_adjusted: {
    label: 'Threshold tuned',
    tone: 'neutral',
    Icon: SlidersHorizontal,
    title: 'The auto-fork threshold was adjusted from recent overflow signals',
  },
  rate_limited: {
    label: 'Rate limited',
    tone: 'warn',
    Icon: Clock,
    title: 'Provider rate limits are slowing responses',
  },
}

export function agentStatusVariant(status: Agent['status']): 'success' | 'warning' | 'error' | 'default' {
  switch (status) {
    case 'online': return 'success'
    case 'working': return 'warning'
    case 'error': return 'error'
    default: return 'default'
  }
}

export function agentStatusLabel(status: Agent['status']): string {
  switch (status) {
    case 'online': return 'Online'
    case 'working': return 'Working'
    case 'error': return 'Error'
    default: return 'Offline'
  }
}

export function getAgentAttentionMeta(attentionState?: AgentAttentionState): AgentAttentionMeta {
  return ATTENTION_META[attentionState ?? 'idle'] ?? ATTENTION_META.idle
}

export function agentAttentionTone(attentionState?: AgentAttentionState): AgentAttentionTone {
  return getAgentAttentionMeta(attentionState).tone
}

export function agentAttentionVariant(attentionState?: AgentAttentionState): 'success' | 'info' | 'warning' | 'error' | 'default' {
  switch (agentAttentionTone(attentionState)) {
    case 'success':
      return 'success'
    case 'info':
      return 'info'
    case 'warn':
      return 'warning'
    case 'danger':
      return 'error'
    default:
      return 'default'
  }
}

export function agentAttentionLabel(attentionState?: AgentAttentionState): string {
  return getAgentAttentionMeta(attentionState).label
}

export function agentAttentionIcon(attentionState?: AgentAttentionState): LucideIcon | undefined {
  return getAgentAttentionMeta(attentionState).Icon
}

export function agentAttentionTitle(attentionState?: AgentAttentionState): string | undefined {
  return getAgentAttentionMeta(attentionState).title
}

export function agentAttentionAnimated(attentionState?: AgentAttentionState): boolean {
  return getAgentAttentionMeta(attentionState).animated ?? false
}

export function taskStatusVariant(status: Task['status']): 'success' | 'warning' | 'info' | 'default' {
  switch (status) {
    case 'completed':
      return 'success'
    case 'claimed':
    case 'in_progress': return 'warning'
    case 'failed':
      return 'default'
    default: return 'default'
  }
}

export function taskStatusLabel(status: Task['status']): string {
  switch (status) {
    case 'pending':
      return 'Pending'
    case 'claimed':
      return 'Claimed'
    case 'in_progress': return 'In Progress'
    case 'completed':
      return 'Completed'
    case 'failed':
      return 'Failed'
    default: return status
  }
}

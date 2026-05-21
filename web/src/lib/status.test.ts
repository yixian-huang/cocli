import { describe, expect, it } from 'vitest'
import { agentAttentionLabel, agentAttentionTitle, agentAttentionTone } from './status'
import type { AgentAttentionState } from './types'

describe('agent attention metadata', () => {
  it('covers the expanded attention-state palette with semantic tones', () => {
    const cases: Array<{
      state: AgentAttentionState
      label: string
      tone: ReturnType<typeof agentAttentionTone>
      title?: string
    }> = [
      { state: 'idle', label: 'Idle', tone: 'neutral' },
      { state: 'working', label: 'Working', tone: 'success' },
      { state: 'focus', label: 'Focus', tone: 'info', title: 'Agent is locked on the current task' },
      {
        state: 'preempting',
        label: 'Preempting',
        tone: 'warn',
        title: 'Higher-priority work is interrupting the current flow',
      },
      { state: 'stalled', label: 'Stalled', tone: 'danger', title: '通知循环被暂停（点击 restart）' },
      {
        state: 'context_pressure',
        label: 'Context high',
        tone: 'warn',
        title: 'Context usage is approaching the fork threshold',
      },
      {
        state: 'context_overflow',
        label: 'Context overflow',
        tone: 'danger',
        title: 'Context window overflow was detected',
      },
      {
        state: 'backstop_threshold_adjusted',
        label: 'Threshold tuned',
        tone: 'neutral',
        title: 'The auto-fork threshold was adjusted from recent overflow signals',
      },
      {
        state: 'rate_limited',
        label: 'Rate limited',
        tone: 'warn',
        title: 'Provider rate limits are slowing responses',
      },
    ]

    for (const testCase of cases) {
      expect(agentAttentionLabel(testCase.state)).toBe(testCase.label)
      expect(agentAttentionTone(testCase.state)).toBe(testCase.tone)
      expect(agentAttentionTitle(testCase.state)).toBe(testCase.title)
    }
  })
})

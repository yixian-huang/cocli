import { useCallback, useEffect, useMemo, useState } from 'react'
import { AlertTriangle, Bot, Shield } from 'lucide-react'

import { channels as channelsApi } from '@/api/client'
import type { ChannelResponderPolicy, ResponderMode, ResponderRole } from '@/lib/types'
import { useAgentStore } from '@/stores/agentStore'
import { toast, toastError } from '@/stores/toastStore'
import { Badge, Select } from '@/components/ui'

interface Member {
  id: string
  memberId: string
  memberType: string
}

interface Props {
  channelId: string
  members: Member[]
}

const modeOptions: { value: ResponderMode; label: string }[] = [
  { value: 'collaborative', label: 'Collaborative (all full)' },
  { value: 'governed', label: 'Governed (primary + tiered)' },
  { value: 'strict', label: 'Strict (primary only)' },
]

const roleOptions: { value: ResponderRole; label: string }[] = [
  { value: 'owner', label: 'Owner' },
  { value: 'backup', label: 'Backup' },
  { value: 'observer', label: 'Observer' },
  { value: 'silent', label: 'Silent' },
]

function fallbackRole(agentCount: number): ResponderRole {
  return agentCount <= 1 ? 'owner' : 'observer'
}

function roleBadgeVariant(role: ResponderRole): 'default' | 'info' | 'warning' {
  switch (role) {
    case 'owner':
      return 'default'
    case 'backup':
      return 'info'
    default:
      return 'warning'
  }
}

export function ChannelResponderPolicyPanel({ channelId, members }: Props) {
  const agents = useAgentStore((s) => s.agents)
  const [mode, setMode] = useState<ResponderMode>('collaborative')
  const [loading, setLoading] = useState(false)
  const [savingMode, setSavingMode] = useState(false)
  const [savingRoleByAgent, setSavingRoleByAgent] = useState<Record<string, boolean>>({})
  const [policyByAgent, setPolicyByAgent] = useState<Record<string, ChannelResponderPolicy>>({})

  const agentMembers = useMemo(
    () =>
      members
        .filter((m) => m.memberType === 'agent')
        .map((m) => {
          const agent = agents.find((a) => a.id === m.memberId)
          return {
            id: m.memberId,
            name: agent?.displayName || agent?.name || `agent:${m.memberId.slice(0, 8)}`,
          }
        })
        .sort((a, b) => a.name.localeCompare(b.name)),
    [agents, members],
  )

  const loadPolicies = useCallback(async () => {
    setLoading(true)
    try {
      const [modeState, policies] = await Promise.all([
        channelsApi.getResponderMode(channelId),
        channelsApi.listResponderPolicies(channelId),
      ])
      setMode(modeState.mode)
      const nextPolicyByAgent: Record<string, ChannelResponderPolicy> = {}
      for (const policy of policies) {
        nextPolicyByAgent[policy.agentId] = policy
      }
      setPolicyByAgent(nextPolicyByAgent)
    } catch (err) {
      toastError(err instanceof Error ? err.message : 'Failed to load responder policy')
    } finally {
      setLoading(false)
    }
  }, [channelId])

  useEffect(() => {
    loadPolicies()
  }, [loadPolicies])

  const handleModeChange = async (nextMode: ResponderMode) => {
    if (nextMode === mode) return
    setSavingMode(true)
    try {
      const updated = await channelsApi.updateResponderMode(channelId, nextMode)
      setMode(updated.mode)
      toast(`Responder mode set to ${updated.mode}`, 'success')
    } catch (err) {
      toastError(err instanceof Error ? err.message : 'Failed to update responder mode')
    } finally {
      setSavingMode(false)
    }
  }

  const handleRoleChange = async (agentId: string, role: ResponderRole) => {
    const currentRole = policyByAgent[agentId]?.role ?? fallbackRole(agentMembers.length)
    if (role === currentRole) return
    setSavingRoleByAgent((s) => ({ ...s, [agentId]: true }))
    try {
      const updated = await channelsApi.upsertResponderPolicy(channelId, agentId, role, 0)
      setPolicyByAgent((s) => ({ ...s, [agentId]: updated }))
      toast(`Role updated to ${role}`, 'success')
    } catch (err) {
      toastError(err instanceof Error ? err.message : 'Failed to update role')
    } finally {
      setSavingRoleByAgent((s) => ({ ...s, [agentId]: false }))
    }
  }

  return (
    <div className="space-y-4">
      <div className="rounded-lg border p-3 space-y-3">
        <div className="flex items-center gap-2 text-sm font-medium">
          <Shield className="h-4 w-4 text-primary" />
          Responder Mode
        </div>
        <Select
          label="Routing Mode"
          value={mode}
          options={modeOptions}
          disabled={loading || savingMode}
          onChange={(e) => handleModeChange(e.target.value as ResponderMode)}
        />
        {mode === 'strict' && (
          <div className="flex items-start gap-2 rounded-md border border-amber-500/40 bg-amber-500/10 p-2 text-xs text-amber-900 dark:text-amber-100">
            <AlertTriangle className="h-3.5 w-3.5 mt-0.5 shrink-0" />
            <p>
              Strict mode is for high-noise channels. Non-primary replies can be denied when strict gate is enabled on
              server.
            </p>
          </div>
        )}
      </div>

      <div className="rounded-lg border p-3 space-y-2">
        <div className="text-sm font-medium">Agent Roles</div>
        {agentMembers.length === 0 ? (
          <p className="text-xs text-muted-foreground">No agent members in this channel.</p>
        ) : (
          agentMembers.map((agentMember) => {
            const role = policyByAgent[agentMember.id]?.role ?? fallbackRole(agentMembers.length)
            return (
              <div key={agentMember.id} className="rounded-md border px-2 py-2 space-y-2">
                <div className="flex items-center gap-2">
                  <Bot className="h-3.5 w-3.5 text-primary" />
                  <span className="text-sm flex-1 truncate">@{agentMember.name}</span>
                  <Badge variant={roleBadgeVariant(role)} size="sm">{role}</Badge>
                </div>
                <Select
                  options={roleOptions}
                  value={role}
                  disabled={loading || !!savingRoleByAgent[agentMember.id]}
                  onChange={(e) => handleRoleChange(agentMember.id, e.target.value as ResponderRole)}
                />
              </div>
            )
          })
        )}
      </div>
    </div>
  )
}

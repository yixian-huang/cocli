import { useState, useEffect } from 'react'
import { useAgentStore } from '@/stores/agentStore'
import { useViewStore } from '@/stores/viewStore'
import { agents as agentsApi } from '@/api/client'
import { toast, toastError } from '@/stores/toastStore'
import { Button, Textarea, Badge } from '@/components/ui'
import { agentStatusVariant, agentStatusLabel } from '@/lib/status'
import { Save, X, Trash2 } from 'lucide-react'
import { cn } from '@/lib/utils'
import { BRAND } from '@/brand'
import type { Machine } from '@/lib/types'

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`
  return String(n)
}

export function ProfileTab({ agentId }: { agentId: string }) {
  const agent = useAgentStore((s) => s.agents.find((a) => a.id === agentId))
  const fetchAgents = useAgentStore((s) => s.fetchAgents)
  const [daemon, setDaemon] = useState<(Machine & { connected: boolean }) | null>(null)
  const [editing, setEditing] = useState(false)
  const [editDesc, setEditDesc] = useState('')
  const [editModel, setEditModel] = useState('')
  const [saving, setSaving] = useState(false)
  const [deleting, setDeleting] = useState(false)

  useEffect(() => {
    // daemons list removed (zone-scoped feature deleted in Phase 1)
    setDaemon(null)
  }, [agent?.machineId])

  if (!agent) return null

  const handleEdit = () => {
    setEditDesc(agent.description || '')
    setEditModel(agent.model)
    setEditing(true)
  }

  const handleSave = async () => {
    setSaving(true)
    try {
      await agentsApi.update(agent.id, { description: editDesc, model: editModel })
      await fetchAgents()
      setEditing(false)
      toast('Agent updated', 'success')
    } catch (err) {
      toastError(err instanceof Error ? err.message : 'Failed to update')
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="flex-1 overflow-y-auto p-4 space-y-6 max-w-lg">
      {/* Basic */}
      <section className="space-y-3">
        <h4 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">Basic</h4>
        <div className="space-y-2 text-sm">
          <Row label="Name" value={`@${agent.name}`} />
          <Row label="Display Name" value={agent.displayName || '—'} />
          {editing ? (
            <div>
              <label className="text-xs text-muted-foreground">Description</label>
              <Textarea value={editDesc} onChange={(e) => setEditDesc(e.target.value)} rows={3} className="mt-1 text-xs" />
            </div>
          ) : (
            <Row label="Description" value={agent.description || '—'} />
          )}
        </div>
      </section>

      {/* Technical */}
      <section className="space-y-3">
        <h4 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">Technical</h4>
        <div className="space-y-2 text-sm">
          <Row label="Runtime" value={agent.runtime} />
          <Row label="Workspace" value={`~/.${BRAND.slug}/agents/${agent.id}`} mono />
          {editing ? (
            <div>
              <label className="text-xs text-muted-foreground">Model</label>
              <select
                value={editModel}
                onChange={(e) => setEditModel(e.target.value)}
                className="mt-1 w-full rounded border bg-background px-2 py-1.5 text-xs focus:outline-none focus:ring-1 focus:ring-ring"
              >
                <option value="claude-sonnet-4-6">Claude Sonnet 4.6</option>
                <option value="claude-opus-4-6">Claude Opus 4.6</option>
                <option value="claude-haiku-4-5-20251001">Claude Haiku 4.5</option>
                <option value="o3">OpenAI o3</option>
                <option value="o4-mini">OpenAI o4-mini</option>
                <option value="gpt-4.1">GPT-4.1</option>
                <option value="gemini-2.5-pro">Gemini 2.5 Pro</option>
                <option value="gemini-2.5-flash">Gemini 2.5 Flash</option>
                {editModel && ![
                  'claude-sonnet-4-6', 'claude-opus-4-6', 'claude-haiku-4-5-20251001',
                  'o3', 'o4-mini', 'gpt-4.1', 'gemini-2.5-pro', 'gemini-2.5-flash'
                ].includes(editModel) && (
                  <option value={editModel}>{editModel}</option>
                )}
              </select>
            </div>
          ) : (
            <Row label="Model" value={agent.model} />
          )}
          <div className="flex items-start gap-3">
            <span className="text-muted-foreground w-24 shrink-0 text-xs">Status</span>
            <Badge variant={agentStatusVariant(agent.status)} size="sm">
              {agentStatusLabel(agent.status)}
            </Badge>
          </div>
          <Row label="Session ID" value={agent.sessionId || '—'} mono />
          <div className="flex items-start gap-3">
            <span className="text-muted-foreground w-24 shrink-0 text-xs">Daemon</span>
            {agent.machineId ? (
              <div className="flex items-center gap-1.5 text-xs">
                <span className={`h-1.5 w-1.5 rounded-full shrink-0 ${daemon?.connected ? 'bg-green-500' : 'bg-red-500'}`} />
                <span>{daemon?.hostname || agent.machineId.slice(0, 12)}</span>
                <span className="text-muted-foreground">({daemon?.connected ? 'online' : 'offline'})</span>
              </div>
            ) : (
              <span className="text-xs text-muted-foreground">Not assigned</span>
            )}
          </div>
        </div>
      </section>

      {/* Timestamps */}
      <section className="space-y-3">
        <h4 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">Timestamps</h4>
        <div className="space-y-2 text-sm">
          <Row label="Created" value={new Date(agent.createdAt).toLocaleString()} />
          <Row label="Updated" value={new Date(agent.updatedAt).toLocaleString()} />
        </div>
      </section>

      {/* Usage */}
      <section className="space-y-3">
        <h4 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">Usage</h4>
        <div className="space-y-2 text-sm">
          <Row label="Turns" value={agent.turnCount != null ? String(agent.turnCount) : '—'} />
          <Row label="Input Tokens" value={agent.lastInputTokens != null ? formatTokens(agent.lastInputTokens) : '—'} />
          <Row label="Output Tokens" value={agent.totalOutputTokens != null ? formatTokens(agent.totalOutputTokens) : '—'} />
          <Row label="Context" value={agent.contextWindow != null && agent.lastInputTokens != null
            ? `${formatTokens(agent.lastInputTokens)} / ${formatTokens(agent.contextWindow)} (${Math.round((agent.lastInputTokens / agent.contextWindow) * 100)}%)`
            : '—'} />
          <Row label="Cost" value={agent.totalCostUSD != null ? `$${agent.totalCostUSD.toFixed(4)}` : '—'} />
        </div>
      </section>

      {/* Actions */}
      <div className="flex gap-2">
        {editing ? (
          <>
            <Button variant="primary" size="sm" onClick={handleSave} disabled={saving}>
              <Save className="h-3 w-3" />{saving ? 'Saving...' : 'Save'}
            </Button>
            <Button variant="secondary" size="sm" onClick={() => setEditing(false)}>
              <X className="h-3 w-3" /> Cancel
            </Button>
          </>
        ) : (
          <Button variant="secondary" size="sm" onClick={handleEdit}>
            Edit Profile
          </Button>
        )}
        <Button
          variant="ghost"
          size="sm"
          className="text-red-500 hover:bg-red-500/10 ml-auto"
          disabled={deleting || agent.status !== 'offline'}
          title={agent.status !== 'offline' ? 'Stop the agent before deleting' : 'Delete agent'}
          onClick={async () => {
            if (!confirm(`Delete @${agent.name}? This cannot be undone.`)) return
            setDeleting(true)
            try {
              await agentsApi.delete(agent.id)
              await fetchAgents()
              useViewStore.getState().setActiveAgent('')
              toast(`@${agent.name} deleted`, 'info')
            } catch (err) {
              toastError(err instanceof Error ? err.message : 'Failed to delete')
            } finally {
              setDeleting(false)
            }
          }}
        >
          <Trash2 className="h-3 w-3" /> {deleting ? 'Deleting...' : 'Delete'}
        </Button>
      </div>
    </div>
  )
}

function Row({ label, value, mono, valueClass }: { label: string; value: string; mono?: boolean; valueClass?: string }) {
  return (
    <div className="flex items-start gap-3">
      <span className="text-muted-foreground w-24 shrink-0 text-xs">{label}</span>
      <span className={cn('text-xs break-all', mono && 'font-mono', valueClass)}>{value}</span>
    </div>
  )
}

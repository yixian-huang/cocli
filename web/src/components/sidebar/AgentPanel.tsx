import { useState } from 'react'
import { useAgentStore } from '@/stores/agentStore'
import { agents as agentsApi } from '@/api/client'
import { toast, toastError } from '@/stores/toastStore'
import { Button, Input, Textarea, StatusDot, AttentionBadge, ConfirmDialog } from '@/components/ui'
import { agentStatusLabel } from '@/lib/status'
import type { Agent } from '@/lib/types'
import { Play, Square, Loader2, Settings2, Save, RefreshCw } from 'lucide-react'
import { ContextBar } from '../agents/ContextBar'

export function AgentPanel({ agent }: { agent: Agent }) {
  const [loading, setLoading] = useState(false)
  const [cancelingTurn, setCancelingTurn] = useState(false)
  const [steeringTurn, setSteeringTurn] = useState(false)
  const [forkingThread, setForkingThread] = useState(false)
  const [steerInput, setSteerInput] = useState('')
  const [cancelTurnDialogOpen, setCancelTurnDialogOpen] = useState(false)
  const [forkThreadDialogOpen, setForkThreadDialogOpen] = useState(false)
  const [editing, setEditing] = useState(false)
  const [editDesc, setEditDesc] = useState(agent.description || '')
  const [editModel, setEditModel] = useState(agent.model)
  const [saving, setSaving] = useState(false)
  // Single-tenant: local owner has full access
  const isAdmin = true
  const startAgent = useAgentStore((s) => s.startAgent)
  const stopAgent = useAgentStore((s) => s.stopAgent)
  const cancelAgentTurn = useAgentStore((s) => s.cancelAgentTurn)
  const steerAgentTurn = useAgentStore((s) => s.steerAgentTurn)
  const fetchAgents = useAgentStore((s) => s.fetchAgents)
  const canManageTurn = isAdmin && agent.status === 'working'
  const supportsTurnControl = agent.runtime === 'codex'
  const unsupportedTurnControlTitle = `Unsupported for ${agent.runtime}`

  const handleStart = async () => {
    setLoading(true)
    try {
      await startAgent(agent.id)
      toast(`@${agent.name} starting...`, 'success')
    } catch (err) {
      toastError(err instanceof Error ? err.message : 'Failed to start agent')
    } finally {
      setLoading(false)
    }
  }

  const handleStop = async () => {
    setLoading(true)
    try {
      await stopAgent(agent.id)
      toast(`@${agent.name} stopping...`, 'info')
    } catch (err) {
      toastError(err instanceof Error ? err.message : 'Failed to stop agent')
    } finally {
      setLoading(false)
    }
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

  const handleCancelTurn = async () => {
    setCancelingTurn(true)
    try {
      await cancelAgentTurn(agent.id)
      setCancelTurnDialogOpen(false)
      toast(`@${agent.name} turn cancellation requested`, 'info')
    } catch (err) {
      toastError(err instanceof Error ? err.message : 'Failed to cancel turn')
    } finally {
      setCancelingTurn(false)
    }
  }

  const handleSteerTurn = async () => {
    if (!supportsTurnControl) return
    const input = steerInput.trim()
    if (!input) return
    setSteeringTurn(true)
    try {
      await steerAgentTurn(agent.id, input)
      setSteerInput('')
      toast('Steer injected', 'success')
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to inject steer'
      if (message.startsWith('409:')) {
        toastError('No active turn')
      } else {
        toastError(message)
      }
    } finally {
      setSteeringTurn(false)
    }
  }

  const handleForkThread = async () => {
    setForkingThread(true)
    try {
      await agentsApi.forkThread(agent.id)
      setForkThreadDialogOpen(false)
      toast('Fork requested', 'success')
    } catch (err) {
      toastError(err instanceof Error ? err.message : 'Failed to fork thread')
    } finally {
      setForkingThread(false)
    }
  }

  return (
    <div className="ml-4 space-y-2 border-l border-border-default px-3 py-2 text-xs">
      <div className="flex items-center gap-1.5 text-muted-foreground">
        <span className="font-medium">Status:</span>
        <StatusDot status={agent.status as 'online' | 'offline' | 'working' | 'error'} size="sm" />
        <span>{agentStatusLabel(agent.status)}</span>
        {agent.attentionState && agent.attentionState !== 'idle' && <AttentionBadge state={agent.attentionState} />}
      </div>
      {agent.detail && (
        <div className="text-muted-foreground truncate">{agent.detail}</div>
      )}

      {editing ? (
        <div className="space-y-1.5">
          <div>
            <label className="text-muted-foreground text-[10px]">Model</label>
            <Input type="text" value={editModel} onChange={(e) => setEditModel(e.target.value)} className="text-xs" />
          </div>
          <div>
            <label className="text-muted-foreground text-[10px]">Description / Instructions</label>
            <Textarea value={editDesc} onChange={(e) => setEditDesc(e.target.value)} rows={3} className="text-xs" />
          </div>
          <div className="flex gap-1.5">
            <Button variant="primary" size="sm" onClick={handleSave} disabled={saving}>
              <Save className="h-3 w-3" />{saving ? 'Saving...' : 'Save'}
            </Button>
            <Button variant="secondary" size="sm" onClick={() => setEditing(false)}>
              Cancel
            </Button>
          </div>
        </div>
      ) : (
        <>
          <div className="text-muted-foreground/70">{agent.runtime} / {agent.model}</div>
          {agent.description && (
            <div className="text-muted-foreground italic">{agent.description}</div>
          )}
        </>
      )}

      {agent.trajectory && agent.trajectory.length > 0 && (
        <div className="space-y-1">
          <span className="font-medium text-muted-foreground">Tools:</span>
          <div className="flex flex-wrap gap-1">
            {agent.trajectory.slice(-8).map((tool, i) => (
              <span key={i} className="inline-block px-1.5 py-0.5 rounded bg-accent text-accent-foreground text-[10px] font-mono">
                {tool}
              </span>
            ))}
          </div>
        </div>
      )}
      {agent.status !== 'offline' && agent.contextWindow && agent.contextWindow > 0 && (
        <ContextBar
          lastInputTokens={agent.lastInputTokens}
          contextWindow={agent.contextWindow}
          totalOutputTokens={agent.totalOutputTokens}
          totalCostUSD={agent.totalCostUSD}
          turnCount={agent.turnCount}
        />
      )}
      <div className="flex flex-wrap items-center gap-1.5 pt-1">
        {agent.status === 'offline' ? (
          <button onClick={handleStart} disabled={loading} title="Start agent"
            className="flex h-6 w-6 items-center justify-center text-success-emphasis transition-colors hover:bg-success/10 disabled:opacity-50">
            {loading ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Play className="h-3.5 w-3.5" />}
          </button>
        ) : (
          <button onClick={handleStop} disabled={loading} title="Stop agent"
            className="flex h-6 w-6 items-center justify-center text-error-emphasis transition-colors hover:bg-error/10 disabled:opacity-50">
            {loading ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Square className="h-3.5 w-3.5" />}
          </button>
        )}
        {canManageTurn && (
          <form
            className="flex min-w-0 flex-1 items-center gap-1.5"
            onSubmit={(e) => {
              e.preventDefault()
              void handleSteerTurn()
            }}
          >
            <Input
              value={steerInput}
              onChange={(e) => setSteerInput(e.target.value)}
              placeholder="Steer..."
              className="h-7 min-w-24 flex-1 text-xs"
              disabled={!supportsTurnControl || steeringTurn}
            />
            <span title={!supportsTurnControl ? unsupportedTurnControlTitle : undefined}>
              <Button
                type="submit"
                variant="secondary"
                size="sm"
                loading={steeringTurn}
                disabled={!supportsTurnControl || !steerInput.trim() || steeringTurn}
              >
                Send
              </Button>
            </span>
            <span title={!supportsTurnControl ? unsupportedTurnControlTitle : undefined}>
              <Button
                type="button"
                variant="danger"
                size="sm"
                onClick={() => setCancelTurnDialogOpen(true)}
                disabled={!supportsTurnControl || cancelingTurn}
              >
                Cancel turn
              </Button>
            </span>
            <Button
              type="button"
              variant="primary"
              size="sm"
              onClick={() => setForkThreadDialogOpen(true)}
              disabled={forkingThread}
            >
              <RefreshCw className="h-3 w-3" />
              Fork now
            </Button>
          </form>
        )}
        {!editing && (
          <button onClick={() => { setEditDesc(agent.description || ''); setEditModel(agent.model); setEditing(true) }}
            className="flex items-center gap-1 px-2 py-1 rounded border hover:bg-accent text-muted-foreground transition-colors">
            <Settings2 className="h-3 w-3" /> Config
          </button>
        )}
      </div>
      <ConfirmDialog
        open={cancelTurnDialogOpen}
        onClose={() => setCancelTurnDialogOpen(false)}
        onConfirm={handleCancelTurn}
        title="Cancel current turn?"
        message="中断会丢当前工作，确认继续吗？"
        confirmLabel="Cancel turn"
        variant="danger"
        loading={cancelingTurn}
      />
      <ConfirmDialog
        open={forkThreadDialogOpen}
        onClose={() => setForkThreadDialogOpen(false)}
        onConfirm={handleForkThread}
        title="Create fresh thread?"
        message="Context will be summarized, in-flight turn interrupted."
        confirmLabel="Fork now"
        variant="primary"
        loading={forkingThread}
      />
    </div>
  )
}

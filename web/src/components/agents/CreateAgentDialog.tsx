import { useState, useEffect } from 'react'
import { Modal, Button } from '@/components/ui'
import { useDialogStore } from '@/stores/dialogStore'
import { useAgentStore } from '@/stores/agentStore'
import { toast } from '@/stores/toastStore'
import * as api from '@/api/client'

export function CreateAgentDialog() {
  const open = useDialogStore((s) => s.active === 'createAgent')
  const close = useDialogStore((s) => s.close)

  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [runtime, setRuntime] = useState('')
  const [model, setModel] = useState('claude-haiku-4-5-20251001')
  const [availableRuntimes, setAvailableRuntimes] = useState<string[]>([])
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (open) {
      api.agents.runtimes().then(setAvailableRuntimes).catch(() => {})
    }
    if (!open) {
      setName('')
      setDescription('')
      setRuntime('')
      setModel('claude-haiku-4-5-20251001')
      setSubmitting(false)
      setError(null)
    }
  }, [open])

  const submit = async () => {
    if (!name.trim()) return
    setSubmitting(true)
    setError(null)
    try {
      await api.agents.create({
        name: name.trim(),
        runtime: runtime || undefined,
        model,
        description: description || undefined,
      })
      await useAgentStore.getState().fetchAgents()
      toast(`@${name.trim()} created`, 'success')
      close()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create agent')
    } finally {
      setSubmitting(false)
    }
  }

  const canSubmit = !!name.trim() && !submitting

  return (
    <Modal
      open={open}
      onClose={close}
      title="Create Agent"
      size="md"
      footer={
        <>
          <Button variant="ghost" onClick={close} disabled={submitting}>Cancel</Button>
          <Button onClick={submit} disabled={!canSubmit}>
            {submitting ? 'Creating…' : 'Create'}
          </Button>
        </>
      }
    >
      <label className="block text-xs uppercase opacity-50 mb-1" htmlFor="agent-name">Name</label>
      <input
        id="agent-name"
        value={name}
        onChange={(e) => setName(e.target.value)}
        className="w-full mb-4 px-3 py-2 rounded bg-muted border border-border text-sm outline-none"
        placeholder="my-agent"
        autoFocus
      />

      {availableRuntimes.length > 0 && (
        <>
          <label className="block text-xs uppercase opacity-50 mb-1">Runtime</label>
          <select
            value={runtime}
            onChange={(e) => setRuntime(e.target.value)}
            className="w-full mb-4 px-3 py-2 rounded bg-muted border border-border text-sm outline-none"
          >
            <option value="">Default</option>
            {availableRuntimes.map((r) => (
              <option key={r} value={r}>{r}</option>
            ))}
          </select>
        </>
      )}

      <label className="block text-xs uppercase opacity-50 mb-1">Model</label>
      <select
        value={model}
        onChange={(e) => setModel(e.target.value)}
        className="w-full mb-4 px-3 py-2 rounded bg-muted border border-border text-sm outline-none"
      >
        <option value="claude-sonnet-4-6">Claude Sonnet 4.6</option>
        <option value="claude-opus-4-6">Claude Opus 4.6</option>
        <option value="claude-haiku-4-5-20251001">Claude Haiku 4.5</option>
        <option value="o3">OpenAI o3</option>
        <option value="o4-mini">OpenAI o4-mini</option>
        <option value="gemini-2.5-pro">Gemini 2.5 Pro</option>
        <option value="gemini-2.5-flash">Gemini 2.5 Flash</option>
      </select>

      <label className="block text-xs uppercase opacity-50 mb-1" htmlFor="agent-description">
        Description (optional)
      </label>
      <textarea
        id="agent-description"
        value={description}
        onChange={(e) => setDescription(e.target.value)}
        className="w-full mb-2 px-3 py-2 rounded bg-muted border border-border text-sm outline-none resize-none"
        rows={2}
      />

      {error && <div className="mt-2 text-sm text-destructive">{error}</div>}
    </Modal>
  )
}

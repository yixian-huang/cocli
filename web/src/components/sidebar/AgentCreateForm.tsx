import { useState } from 'react'
import { useAgentStore } from '@/stores/agentStore'
import { agents as agentsApi } from '@/api/client'
import { toast, toastError } from '@/stores/toastStore'
import { Button, Input, Textarea, Select } from '@/components/ui'

const RUNTIMES = ['claude', 'codex', 'gemini']

const DEFAULT_MODEL: Record<string, string> = {
  claude: 'sonnet',
  gemini: 'gemini-2.5-pro',
  codex: 'o3',
}

export function AgentCreateForm({ onClose }: { onClose: () => void }) {
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [runtime, setRuntime] = useState('claude')
  const [model, setModel] = useState(DEFAULT_MODEL['claude'])
  const [customModel, setCustomModel] = useState('')
  const [creating, setCreating] = useState(false)
  const fetchAgents = useAgentStore((s) => s.fetchAgents)

  const selectedModel = model === '__custom__' ? customModel : model

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault()
    const trimmed = name.trim().toLowerCase().replace(/[^a-z0-9-_]/g, '-')
    if (!trimmed || !selectedModel) return
    setCreating(true)
    try {
      await agentsApi.create({
        name: trimmed,
        runtime,
        model: selectedModel,
        description: description.trim() || undefined,
      })
      await fetchAgents()
      onClose()
      toast(`@${trimmed} created`, 'success')
    } catch (err) {
      toastError(err instanceof Error ? err.message : 'Failed to create agent')
    } finally {
      setCreating(false)
    }
  }

  return (
    <form onSubmit={handleCreate} className="px-3 pb-2 space-y-1.5">
      <Input
        type="text" value={name} onChange={(e) => setName(e.target.value)}
        placeholder="agent-name" autoFocus disabled={creating}
        className="text-xs"
      />

      {/* Runtime selector */}
      <Select
        value={runtime}
        onChange={(e) => {
          setRuntime(e.target.value)
          setModel(DEFAULT_MODEL[e.target.value] || '')
          setCustomModel('')
        }}
        disabled={creating}
        options={RUNTIMES.map((r) => ({ value: r, label: r }))}
        className="text-xs"
      />

      {/* Model selector */}
      <Select
        value={model}
        onChange={(e) => setModel(e.target.value)}
        disabled={creating}
        options={[
          { value: DEFAULT_MODEL[runtime] || '', label: DEFAULT_MODEL[runtime] || 'default' },
          { value: '__custom__', label: 'Custom model...' },
        ]}
        className="text-xs"
      />
      {model === '__custom__' && (
        <Input
          type="text" value={customModel} onChange={(e) => setCustomModel(e.target.value)}
          placeholder="model name (e.g. claude-opus-4-6)"
          disabled={creating}
          className="text-xs"
        />
      )}

      <Textarea
        value={description} onChange={(e) => setDescription(e.target.value)}
        placeholder="Description / instructions (optional)"
        disabled={creating} rows={2}
        className="text-xs"
      />
      <Button type="submit" variant="primary" className="w-full" disabled={!name.trim() || !selectedModel || creating}>
        {creating ? 'Creating...' : 'Create Agent'}
      </Button>
    </form>
  )
}

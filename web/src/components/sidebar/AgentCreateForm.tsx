import { useState, useEffect } from 'react'
import { useAgentStore } from '@/stores/agentStore'
import { useZoneStore } from '@/stores/zoneStore'
import { agents as agentsApi, daemons as daemonsApi } from '@/api/client'
import { toast, toastError } from '@/stores/toastStore'
import { Button, Input, Textarea, Select } from '@/components/ui'
import type { Machine } from '@/lib/types'

const DEFAULT_MODEL: Record<string, string> = {
  claude: 'sonnet',
  gemini: 'gemini-2.5-pro',
  codex: 'o3',
}

export function AgentCreateForm({ onClose }: { onClose: () => void }) {
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [runtime, setRuntime] = useState('')
  const [model, setModel] = useState('')
  const [customModel, setCustomModel] = useState('')
  const [machines, setMachines] = useState<(Machine & { connected: boolean })[]>([])
  const [selectedDaemon, setSelectedDaemon] = useState('')
  const [creating, setCreating] = useState(false)
  const fetchAgents = useAgentStore((s) => s.fetchAgents)

  useEffect(() => {
    const zoneId = useZoneStore.getState().activeZoneId
    if (!zoneId) return
    daemonsApi.list(zoneId).then(setMachines).catch(() => {})
  }, [])

  const daemon = machines.find((d) => d.id === selectedDaemon)
  const daemonRuntimes = daemon?.runtimes || []

  // Reset runtime/model when daemon changes
  useEffect(() => {
    if (daemonRuntimes.length > 0) {
      const base = daemonRuntimes[0].split('/')[0]
      setRuntime(base)
      setModel(DEFAULT_MODEL[base] || '')
    } else {
      setRuntime('')
      setModel('')
    }
    setCustomModel('')
  }, [selectedDaemon]) // eslint-disable-line react-hooks/exhaustive-deps

  // Reset model when runtime changes
  useEffect(() => {
    setModel(DEFAULT_MODEL[runtime] || '')
    setCustomModel('')
  }, [runtime])

  // Derive available runtimes and models from selected daemon
  const availableRuntimes = [...new Set(daemonRuntimes.map((r) => r.split('/')[0]))]
  const daemonModels = daemon?.models?.[runtime] || []
  const selectedModel = model === '__custom__' ? customModel : model
  const connectedMachines = machines.filter((m) => m.connected)

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault()
    const trimmed = name.trim().toLowerCase().replace(/[^a-z0-9-_]/g, '-')
    if (!trimmed || !selectedModel || !selectedDaemon) return
    setCreating(true)
    try {
      const zoneId = useZoneStore.getState().activeZoneId
      if (!zoneId) return
      await agentsApi.create(zoneId, {
        name: trimmed,
        runtime,
        model: selectedModel,
        description: description.trim() || undefined,
        machineId: selectedDaemon,
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

      {/* Daemon selector */}
      <Select
        value={selectedDaemon}
        onChange={(e) => setSelectedDaemon(e.target.value)}
        disabled={creating}
        options={[
          { value: '', label: connectedMachines.length === 0 ? 'No daemons online' : 'Select daemon...' },
          ...connectedMachines.map((d) => ({
            value: d.id,
            label: `${d.hostname || d.id.slice(0, 8)} — ${(d.runtimes || []).join(', ')}`,
          })),
        ]}
        className="text-xs"
      />

      {/* Runtime selector (from daemon's detected runtimes) */}
      {selectedDaemon && availableRuntimes.length > 0 && (
        <Select
          value={runtime} onChange={(e) => setRuntime(e.target.value)}
          disabled={creating}
          options={availableRuntimes.map((r) => ({ value: r, label: r }))}
          className="text-xs"
        />
      )}

      {/* Model selector */}
      {runtime && (
        <Select
          value={model} onChange={(e) => setModel(e.target.value)}
          disabled={creating}
          options={[
            ...daemonModels.map((m) => ({ value: m.id, label: m.label })),
            { value: '__custom__', label: 'Custom model...' },
          ]}
          className="text-xs"
        />
      )}
      {model === '__custom__' && (
        <Input
          type="text" value={customModel} onChange={(e) => setCustomModel(e.target.value)}
          placeholder="model name (e.g. claude-opus-4-6[1m])"
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
      <Button type="submit" variant="primary" className="w-full" disabled={!name.trim() || !selectedModel || !selectedDaemon || creating}>
        {creating ? 'Creating...' : 'Create Agent'}
      </Button>
    </form>
  )
}

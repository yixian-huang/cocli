import { useState, useEffect } from 'react'
import { Modal, Button } from '@/components/ui'
import { useDialogStore } from '@/stores/dialogStore'
import { useAgentStore } from '@/stores/agentStore'
import { toast } from '@/stores/toastStore'
import * as api from '@/api/client'
import type { Machine, TenantProviderKey } from '@/lib/types'

// Cocli provider profiles — mirrors chatrs's provider catalog.
// The profile name maps to the Rust ProviderProfile.name field.
const CHATRS_PROFILES = [
  { name: 'anthropic', label: 'Anthropic', apiMode: 'anthropic_messages' as const },
  { name: 'openai', label: 'OpenAI', apiMode: 'chat_completions' as const },
  { name: 'deepseek', label: 'DeepSeek', apiMode: 'chat_completions' as const },
  { name: 'kimi', label: 'Kimi (Moonshot)', apiMode: 'chat_completions' as const },
  { name: 'glm', label: 'GLM (ZhipuAI)', apiMode: 'chat_completions' as const },
  { name: 'qwen', label: 'Qwen (Alibaba)', apiMode: 'chat_completions' as const },
  { name: 'openai_compat_custom', label: 'OpenAI-compat (custom)', apiMode: 'chat_completions' as const },
]

export function CreateAgentDialog() {
  const open = useDialogStore((s) => s.active === 'createAgent')
  const payload = useDialogStore((s) => s.payload)
  const close = useDialogStore((s) => s.close)
  const zoneId = (payload as { zoneId?: string } | null)?.zoneId

  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [machines, setMachines] = useState<(Machine & { connected: boolean })[]>([])
  const [selectedDaemon, setSelectedDaemon] = useState<string | null>(null)
  const [selectedRuntime, setSelectedRuntime] = useState<string | null>(null)
  const [model, setModel] = useState('sonnet')
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [agentMode, setAgentMode] = useState<'standard' | 'orchestrator'>('standard')
  const [workingRuntime, setWorkingRuntime] = useState('')
  const [workingModel, setWorkingModel] = useState('')

  // Cocli-specific state
  const [chatrsProfile, setCocliProfile] = useState('anthropic')
  const [chatrsModel, setCocliModel] = useState('claude-haiku-4-5')
  const [chatrsKeyName, setCocliKeyName] = useState('')
  const [chatrsWriteEnabled, setCocliWriteEnabled] = useState(false)
  const [chatrsCredentials, setCocliCredentials] = useState<TenantProviderKey[]>([])

  useEffect(() => {
    if (open && zoneId) {
      api.daemons.list(zoneId).then(setMachines).catch(() => {})
    }
    if (!open) {
      setName('')
      setDescription('')
      setMachines([])
      setSelectedDaemon(null)
      setSelectedRuntime(null)
      setModel('sonnet')
      setSubmitting(false)
      setError(null)
      setAgentMode('standard')
      setWorkingRuntime('')
      setWorkingModel('')
      // Reset chatrs state
      setCocliProfile('anthropic')
      setCocliModel('claude-haiku-4-5')
      setCocliKeyName('')
      setCocliWriteEnabled(false)
      setCocliCredentials([])
    }
  }, [open, zoneId])

  // Load chatrs credentials when runtime switches to chatrs
  useEffect(() => {
    if (selectedRuntime === 'chatrs' && zoneId) {
      api.chatrsCredentials.list(zoneId)
        .then(setCocliCredentials)
        .catch(() => setCocliCredentials([]))
    }
  }, [selectedRuntime, zoneId])

  // Auto-select first available key when credentials load
  useEffect(() => {
    if (chatrsCredentials.length > 0 && !chatrsKeyName) {
      setCocliKeyName(chatrsCredentials[0].name)
    }
  }, [chatrsCredentials, chatrsKeyName])

  const daemon = machines.find((d) => d.id === selectedDaemon)
  const availableRuntimes = daemon?.runtimes || []

  const handleModeChange = (mode: 'standard' | 'orchestrator') => {
    setAgentMode(mode)
    if (mode === 'standard') {
      setWorkingRuntime('')
      setWorkingModel('')
    }
  }

  const isCocli = selectedRuntime === 'chatrs'
  const selectedChatrsProfile = CHATRS_PROFILES.find((p) => p.name === chatrsProfile)

  const submit = async () => {
    if (!zoneId || !name.trim() || !selectedDaemon || !selectedRuntime) return
    if (isCocli && !chatrsKeyName) return
    setSubmitting(true)
    setError(null)
    try {
      const agent = await api.agents.create(zoneId, {
        name: name.trim(),
        runtime: selectedRuntime,
        model: isCocli ? chatrsModel : model,
        description: description || undefined,
        machineId: selectedDaemon,
        workingRuntime:
          agentMode === 'orchestrator' && workingRuntime ? workingRuntime : undefined,
        workingModel: agentMode === 'orchestrator' && workingModel ? workingModel : undefined,
      })

      // For chatrs agents, also set the provider binding
      if (isCocli && selectedChatrsProfile) {
        await api.chatrsAgentBinding.upsert(agent.id, {
          profileName: chatrsProfile,
          model: chatrsModel,
          apiMode: selectedChatrsProfile.apiMode,
          keyName: chatrsKeyName,
          writeEnabled: chatrsWriteEnabled,
        })
      }

      await useAgentStore.getState().fetchAgents()
      toast(`@${name.trim()} created`, 'success')
      close()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create agent')
    } finally {
      setSubmitting(false)
    }
  }

  const canSubmit =
    !!name.trim() &&
    !!selectedDaemon &&
    !!selectedRuntime &&
    !submitting &&
    (!isCocli || !!chatrsKeyName)

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

      <label className="block text-xs uppercase opacity-50 mb-1">Daemon (Machine)</label>
      <div className="mb-4 space-y-1">
        {machines.map((d) => (
          <button
            key={d.id}
            type="button"
            disabled={!d.connected}
            onClick={() => {
              setSelectedDaemon(d.id)
              setSelectedRuntime(null)
            }}
            className={`w-full flex items-center gap-2 px-3 py-2 rounded border text-sm text-left transition-colors ${
              selectedDaemon === d.id ? 'border-primary bg-primary/10' : 'border-border hover:bg-muted'
            } ${!d.connected ? 'opacity-40 cursor-not-allowed' : ''}`}
          >
            <div className={`w-2 h-2 rounded-full ${d.connected ? 'bg-green-500' : 'bg-red-500'}`} />
            <div className="flex-1 min-w-0">
              <div className="font-medium truncate">
                {d.hostname || 'Unknown'}{' '}
                <span className="text-xs font-normal opacity-40">{d.id.slice(0, 8)}</span>
              </div>
              <div className="text-xs opacity-50 truncate">
                {d.os} {d.environment?.memory ? `· ${d.environment.memory}` : ''} ·{' '}
                {(d.runtimes || []).join(', ')}
              </div>
            </div>
          </button>
        ))}
        {machines.length === 0 && (
          <div className="text-sm opacity-50 py-2">No daemons in this zone.</div>
        )}
      </div>

      {selectedDaemon && (
        <>
          <label className="block text-xs uppercase opacity-50 mb-1">Driver</label>
          <div className="flex gap-2 mb-4 flex-wrap">
            {['claude', 'codex', 'gemini', 'kimi', 'chatrs'].map((rt) => {
              const available = availableRuntimes.some((r) => r.startsWith(rt))
              return (
                <button
                  key={rt}
                  type="button"
                  disabled={!available}
                  onClick={() => setSelectedRuntime(rt)}
                  className={`flex-1 py-2 rounded border text-sm text-center transition-colors min-w-[4rem] ${
                    selectedRuntime === rt
                      ? 'border-primary bg-primary/10 font-medium'
                      : 'border-border'
                  } ${!available ? 'opacity-30 cursor-not-allowed' : 'hover:bg-muted'}`}
                >
                  {rt}
                </button>
              )
            })}
          </div>
        </>
      )}

      {selectedRuntime && !isCocli && (
        <>
          <label className="block text-xs uppercase opacity-50 mb-1">Model</label>
          <select
            value={model}
            onChange={(e) => setModel(e.target.value)}
            className="w-full mb-4 px-3 py-2 rounded bg-muted border border-border text-sm outline-none"
          >
            <option value="haiku">haiku</option>
            <option value="sonnet">sonnet</option>
            <option value="opus">opus</option>
          </select>
        </>
      )}

      {isCocli && (
        <div className="mb-4 space-y-3 p-3 rounded border border-border bg-muted/30">
          <div className="text-xs font-medium opacity-70 uppercase tracking-wide">Cocli Provider</div>

          <div>
            <label className="block text-xs opacity-50 mb-1">Provider Profile</label>
            <select
              value={chatrsProfile}
              onChange={(e) => setCocliProfile(e.target.value)}
              className="w-full px-3 py-2 rounded bg-background border border-border text-sm outline-none"
            >
              {CHATRS_PROFILES.map((p) => (
                <option key={p.name} value={p.name}>{p.label}</option>
              ))}
            </select>
          </div>

          <div>
            <label className="block text-xs opacity-50 mb-1">Model</label>
            <input
              value={chatrsModel}
              onChange={(e) => setCocliModel(e.target.value)}
              className="w-full px-3 py-2 rounded bg-background border border-border text-sm outline-none"
              placeholder="e.g. claude-haiku-4-5"
            />
          </div>

          <div>
            <label className="block text-xs opacity-50 mb-1">Provider Key</label>
            {chatrsCredentials.length > 0 ? (
              <select
                value={chatrsKeyName}
                onChange={(e) => setCocliKeyName(e.target.value)}
                className="w-full px-3 py-2 rounded bg-background border border-border text-sm outline-none"
              >
                <option value="">Select a key…</option>
                {chatrsCredentials.map((k) => (
                  <option key={k.id} value={k.name}>
                    {k.name} ({k.profileName})
                  </option>
                ))}
              </select>
            ) : (
              <div className="text-xs text-muted-foreground py-1">
                No provider keys found. Add one in Sidebar → Provider Keys.
              </div>
            )}
          </div>

          <label className="flex items-center gap-2 text-sm cursor-pointer">
            <input
              type="checkbox"
              checked={chatrsWriteEnabled}
              onChange={(e) => setCocliWriteEnabled(e.target.checked)}
              className="border-border"
            />
            <span>Write enabled</span>
            <span className="text-xs text-muted-foreground">— allow agent to write files in workspace (off by default)</span>
          </label>
        </div>
      )}

      <label className="block text-xs uppercase opacity-50 mb-1">Agent Mode</label>
      <div className="space-y-1.5 mb-3">
        <label className="flex items-center gap-2 text-sm cursor-pointer">
          <input
            type="radio"
            name="agentMode"
            checked={agentMode === 'standard'}
            onChange={() => handleModeChange('standard')}
            className="border-border"
          />
          <span>Standard</span>
          <span className="text-xs text-muted-foreground">— Executes code and tasks directly</span>
        </label>
        <label className="flex items-center gap-2 text-sm cursor-pointer">
          <input
            type="radio"
            name="agentMode"
            checked={agentMode === 'orchestrator'}
            onChange={() => handleModeChange('orchestrator')}
            className="border-border"
          />
          <span>Orchestrator</span>
          <span className="text-xs text-muted-foreground">— Coordinates work, delegates execution to subagents</span>
        </label>
      </div>

      {agentMode === 'orchestrator' && (
        <div className="ml-6 space-y-3 border-l-2 border-border pl-4 mb-3">
          <div>
            <label className="text-xs text-muted-foreground block mb-1">Working Runtime</label>
            <select
              value={workingRuntime}
              onChange={(e) => {
                setWorkingRuntime(e.target.value)
                setWorkingModel('')
              }}
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm"
            >
              <option value="">Select runtime</option>
              {availableRuntimes.map((r: string) => (
                <option key={r} value={r}>
                  {r}
                </option>
              ))}
            </select>
          </div>
          {workingRuntime && (
            <div>
              <label className="text-xs text-muted-foreground block mb-1">Working Model</label>
              <select
                value={workingModel}
                onChange={(e) => setWorkingModel(e.target.value)}
                className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm"
              >
                <option value="">Select model</option>
                {['haiku', 'sonnet', 'opus'].map((m) => (
                  <option key={m} value={m}>
                    {m}
                  </option>
                ))}
              </select>
            </div>
          )}
        </div>
      )}

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

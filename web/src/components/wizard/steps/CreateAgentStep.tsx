import { useWizardStore, type Model } from '@/stores/wizardStore'

const MODELS: { id: Model; label: string }[] = [
  { id: 'claude-sonnet-4-6', label: 'Claude Sonnet 4.6 (recommended)' },
  { id: 'claude-haiku-4-5', label: 'Claude Haiku 4.5 (fast)' },
  { id: 'claude-opus-4-7', label: 'Claude Opus 4.7 (most capable)' },
]

function normaliseName(raw: string): string {
  let s = raw.toLowerCase().replace(/[^a-z0-9@-]/g, '')
  if (s.startsWith('@')) s = '@' + s.slice(1).replace(/@/g, '')
  else s = '@' + s.replace(/@/g, '')
  return s === '@' ? '' : s
}

export function CreateAgentStep() {
  const draft = useWizardStore((s) => s.draftAgent)
  const setDraftAgent = useWizardStore((s) => s.setDraftAgent)
  const next = useWizardStore((s) => s.next)
  const back = useWizardStore((s) => s.back)
  const canAdvance = draft.name.length > 1

  return (
    <div className="space-y-5">
      <div>
        <h2 className="text-lg font-medium text-foreground">Create your first agent</h2>
        <p className="mt-1 text-sm text-content-secondary">
          This agent lives on your machine. You can change the model later.
        </p>
      </div>
      <div className="space-y-2">
        <label htmlFor="agent-name" className="text-sm font-medium text-foreground">
          Agent name
        </label>
        <input
          id="agent-name"
          type="text"
          value={draft.name}
          onChange={(e) => setDraftAgent({ name: normaliseName(e.target.value) })}
          placeholder="@assistant"
          className="w-full h-9 px-3 rounded border bg-background text-sm"
        />
      </div>
      <div className="space-y-2">
        <label htmlFor="agent-model" className="text-sm font-medium text-foreground">
          Model
        </label>
        <select
          id="agent-model"
          value={draft.model}
          onChange={(e) => setDraftAgent({ model: e.target.value as Model })}
          className="w-full h-9 px-2 rounded border bg-background text-sm"
        >
          {MODELS.map((m) => (
            <option key={m.id} value={m.id}>{m.label}</option>
          ))}
        </select>
      </div>
      <div className="flex justify-between pt-2">
        <button
          type="button"
          onClick={() => back()}
          className="h-9 px-4 rounded border bg-background text-sm hover:bg-accent"
        >
          Back
        </button>
        <button
          type="button"
          onClick={() => next()}
          disabled={!canAdvance}
          className="h-9 px-4 rounded bg-primary text-primary-foreground text-sm hover:bg-primary/90 disabled:opacity-40 disabled:cursor-not-allowed"
        >
          Next
        </button>
      </div>
    </div>
  )
}

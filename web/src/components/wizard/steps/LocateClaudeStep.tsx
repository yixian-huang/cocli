import { useState } from 'react'
import { Check, Loader2 } from 'lucide-react'
import { useWizardStore } from '@/stores/wizardStore'

export function LocateClaudeStep() {
  const claudePath = useWizardStore((s) => s.claudePath)
  const detectedAt = useWizardStore((s) => s.detectedAt)
  const setClaudePath = useWizardStore((s) => s.setClaudePath)
  const detectClaudePath = useWizardStore((s) => s.detectClaudePath)
  const next = useWizardStore((s) => s.next)
  const [detecting, setDetecting] = useState(false)

  async function handleDetect() {
    setDetecting(true)
    try {
      await detectClaudePath()
    } finally {
      setDetecting(false)
    }
  }

  return (
    <div className="space-y-5">
      <div>
        <h2 className="text-lg font-medium text-foreground">Where is your Claude CLI?</h2>
        <p className="mt-1 text-sm text-content-secondary">
          Leave blank for now — we'll auto-detect when the binary is ready in M0.0.2.
        </p>
      </div>
      <div className="space-y-2">
        <label htmlFor="claude-path" className="text-sm font-medium text-foreground">
          Path to Claude CLI
        </label>
        <div className="flex gap-2">
          <input
            id="claude-path"
            type="text"
            value={claudePath}
            onChange={(e) => setClaudePath(e.target.value)}
            placeholder="/usr/local/bin/claude"
            className="flex-1 h-9 px-3 rounded border bg-background text-sm"
          />
          <button
            type="button"
            onClick={handleDetect}
            disabled={detecting}
            className="h-9 px-3 rounded border bg-background text-sm hover:bg-accent disabled:opacity-50"
          >
            {detecting ? <Loader2 className="h-4 w-4 animate-spin" /> : 'Detect'}
          </button>
          {detectedAt && !detecting && (
            <span data-testid="detect-success" className="h-9 inline-flex items-center text-success">
              <Check className="h-4 w-4" />
            </span>
          )}
        </div>
      </div>
      <div className="flex justify-end pt-2">
        <button
          type="button"
          onClick={() => next()}
          className="h-9 px-4 rounded bg-primary text-primary-foreground text-sm hover:bg-primary/90"
        >
          Next
        </button>
      </div>
    </div>
  )
}

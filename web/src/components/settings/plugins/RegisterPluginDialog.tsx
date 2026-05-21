import { useState } from 'react'
import { usePluginsStore } from '@/stores/pluginsStore'
import type { PluginCapability } from '@shared/types'

const CAPS: PluginCapability[] = ['inbound-bridge', 'outbound-bridge']

function normaliseName(raw: string): string {
  return raw.toLowerCase().replace(/[^a-z0-9-]/g, '').slice(0, 64)
}

export function RegisterPluginDialog({
  open,
  onClose,
  onRegistered,
}: {
  open: boolean
  onClose: () => void
  onRegistered: (token: string) => void
}) {
  const register = usePluginsStore((s) => s.register)
  const [name, setName] = useState('')
  const [selected, setSelected] = useState<PluginCapability[]>([])
  const [submitting, setSubmitting] = useState(false)

  if (!open) return null

  const canSubmit = name.length > 0 && selected.length > 0 && !submitting

  function toggle(cap: PluginCapability) {
    setSelected((s) => (s.includes(cap) ? s.filter((x) => x !== cap) : [...s, cap]))
  }

  async function handleSubmit() {
    if (!canSubmit) return
    setSubmitting(true)
    try {
      const { token } = await register(name, selected)
      setName('')
      setSelected([])
      onRegistered(token)
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-background/70 backdrop-blur-sm">
      <div role="dialog" aria-modal="true" className="w-[440px] max-w-[92vw] rounded-lg border bg-card shadow-2xl">
        <header className="px-5 pt-5 pb-3 border-b">
          <h2 className="text-base font-semibold text-foreground">Register plugin</h2>
        </header>
        <div className="px-5 py-4 space-y-4">
          <div className="space-y-2">
            <label htmlFor="plugin-name" className="text-sm font-medium text-foreground">
              Plugin name
            </label>
            <input
              id="plugin-name"
              type="text"
              value={name}
              onChange={(e) => setName(normaliseName(e.target.value))}
              placeholder="telegram-bot"
              className="w-full h-9 px-3 rounded border bg-background text-sm font-mono"
            />
          </div>
          <fieldset className="space-y-2">
            <legend className="text-sm font-medium text-foreground">Capabilities</legend>
            {CAPS.map((cap) => (
              <label key={cap} className="flex items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  checked={selected.includes(cap)}
                  onChange={() => toggle(cap)}
                />
                <span>{cap}</span>
              </label>
            ))}
          </fieldset>
        </div>
        <footer className="px-5 py-3 border-t flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            className="h-9 px-4 rounded border bg-background text-sm hover:bg-accent"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={handleSubmit}
            disabled={!canSubmit}
            className="h-9 px-4 rounded bg-primary text-primary-foreground text-sm hover:bg-primary/90 disabled:opacity-40 disabled:cursor-not-allowed"
          >
            Register
          </button>
        </footer>
      </div>
    </div>
  )
}

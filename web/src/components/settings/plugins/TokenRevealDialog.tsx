import { Copy } from 'lucide-react'

export function TokenRevealDialog({
  token,
  onClose,
}: {
  token: string | null
  onClose: () => void
}) {
  if (!token) return null
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-background/70 backdrop-blur-sm">
      <div role="dialog" aria-modal="true" className="w-[480px] max-w-[92vw] rounded-lg border bg-card shadow-2xl">
        <header className="px-5 pt-5 pb-3 border-b">
          <h2 className="text-base font-semibold text-foreground">Plugin registered</h2>
        </header>
        <div className="px-5 py-4 space-y-4">
          <div className="rounded border bg-muted p-3 font-mono text-sm break-all">
            {token}
          </div>
          <button
            type="button"
            onClick={() => navigator.clipboard?.writeText(token)}
            className="inline-flex items-center gap-1.5 h-8 px-3 rounded border bg-background text-sm hover:bg-accent"
          >
            <Copy className="h-3.5 w-3.5" /> Copy
          </button>
          <p className="text-sm text-warning bg-warning/10 border border-warning/30 rounded p-3">
            Save this token — it won't be shown again. If you lose it, revoke the plugin and register a new one.
          </p>
        </div>
        <footer className="px-5 py-3 border-t flex justify-end">
          <button
            type="button"
            onClick={onClose}
            className="h-9 px-4 rounded bg-primary text-primary-foreground text-sm hover:bg-primary/90"
          >
            I've saved it
          </button>
        </footer>
      </div>
    </div>
  )
}

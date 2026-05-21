import type { Plugin } from '@shared/types'

export function RevokeConfirmDialog({
  plugin,
  onClose,
  onConfirm,
}: {
  plugin: Plugin | null
  onClose: () => void
  onConfirm: () => void
}) {
  if (!plugin) return null
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-background/70 backdrop-blur-sm">
      <div role="dialog" aria-modal="true" className="w-[400px] max-w-[92vw] rounded-lg border bg-card shadow-2xl">
        <div className="px-5 py-5 space-y-3">
          <h2 className="text-base font-semibold text-foreground">Revoke plugin</h2>
          <p className="text-sm text-content-secondary">
            Revoke <span className="font-mono">{plugin.name}</span>? Connected bridges will disconnect.
          </p>
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
            onClick={onConfirm}
            className="h-9 px-4 rounded bg-destructive text-destructive-foreground text-sm hover:bg-destructive/90"
          >
            Revoke
          </button>
        </footer>
      </div>
    </div>
  )
}

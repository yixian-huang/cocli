import type { Plugin } from '@shared/types'
export function RevokeConfirmDialog({ plugin, onClose: _onClose, onConfirm: _onConfirm }: { plugin: Plugin | null; onClose: () => void; onConfirm: () => void }) {
  if (!plugin) return null
  return <div data-testid="revoke-dialog" />
}

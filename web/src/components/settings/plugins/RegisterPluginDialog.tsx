export function RegisterPluginDialog({ open }: { open: boolean; onClose: () => void; onRegistered: (token: string) => void }) {
  if (!open) return null
  return <div data-testid="register-dialog" />
}

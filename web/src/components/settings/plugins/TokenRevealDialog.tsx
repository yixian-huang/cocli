export function TokenRevealDialog({ token, onClose: _onClose }: { token: string | null; onClose: () => void }) {
  if (!token) return null
  return <div data-testid="token-reveal-dialog">{token}</div>
}

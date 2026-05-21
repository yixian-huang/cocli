import { useEffect, useState } from 'react'
import { Puzzle } from 'lucide-react'
import { usePluginsStore } from '@/stores/pluginsStore'
import { PluginRow } from './PluginRow'
import { RegisterPluginDialog } from './RegisterPluginDialog'
import { TokenRevealDialog } from './TokenRevealDialog'
import { RevokeConfirmDialog } from './RevokeConfirmDialog'
import type { Plugin } from '@shared/types'

export function PluginsPage() {
  const plugins = usePluginsStore((s) => s.plugins)
  const init = usePluginsStore((s) => s.init)
  const revoke = usePluginsStore((s) => s.revoke)
  const [registerOpen, setRegisterOpen] = useState(false)
  const [revealToken, setRevealToken] = useState<string | null>(null)
  const [revokeTarget, setRevokeTarget] = useState<Plugin | null>(null)

  useEffect(() => {
    // Only hydrate from localStorage when the store hasn't been pre-seeded
    // (covers cold-start navigation; tests can pre-seed via setState).
    if (plugins.length === 0) init()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  return (
    <div className="max-w-3xl mx-auto p-6 space-y-6">
      <header className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-semibold text-foreground">Plugins</h1>
          <p className="mt-1 text-sm text-content-secondary">
            Bridge external services into your channels.
          </p>
        </div>
        <button
          type="button"
          onClick={() => setRegisterOpen(true)}
          className="h-9 px-4 rounded bg-primary text-primary-foreground text-sm font-medium hover:bg-primary/90"
        >
          Register plugin
        </button>
      </header>

      {plugins.length === 0 ? (
        <div className="flex flex-col items-center text-center py-16 space-y-3">
          <Puzzle className="h-12 w-12 text-content-secondary/50" />
          <p className="text-content-secondary">No plugins yet</p>
          <p className="text-sm text-content-secondary/70 max-w-md">
            Register one to bridge Telegram, Slack, Discord, or your own custom bridge into a cocli channel.
          </p>
        </div>
      ) : (
        <ul className="border rounded bg-card">
          {plugins.map((p) => (
            <PluginRow key={p.id} plugin={p} onRevoke={() => setRevokeTarget(p)} />
          ))}
        </ul>
      )}

      <RegisterPluginDialog
        open={registerOpen}
        onClose={() => setRegisterOpen(false)}
        onRegistered={(token) => {
          setRegisterOpen(false)
          setRevealToken(token)
        }}
      />
      <TokenRevealDialog token={revealToken} onClose={() => setRevealToken(null)} />
      <RevokeConfirmDialog
        plugin={revokeTarget}
        onClose={() => setRevokeTarget(null)}
        onConfirm={async () => {
          if (revokeTarget) await revoke(revokeTarget.id)
          setRevokeTarget(null)
        }}
      />
    </div>
  )
}

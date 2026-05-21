import { Trash2 } from 'lucide-react'
import type { Plugin } from '@shared/types'

const capColor: Record<string, string> = {
  'inbound-bridge': 'bg-success/15 text-success',
  'outbound-bridge': 'bg-info/15 text-info',
}

export function PluginRow({ plugin, onRevoke }: { plugin: Plugin; onRevoke: () => void }) {
  return (
    <li className="flex items-center justify-between px-4 py-3 border-b last:border-b-0">
      <div className="flex flex-col gap-1 min-w-0">
        <span className="font-mono text-sm text-foreground truncate">{plugin.name}</span>
        <div className="flex flex-wrap items-center gap-1.5 text-xs">
          {plugin.capabilities.map((c) => (
            <span
              key={c}
              className={`px-1.5 py-0.5 rounded ${capColor[c] ?? 'bg-muted text-muted-foreground'}`}
            >
              {c}
            </span>
          ))}
          <span className="text-content-secondary">
            • Last seen: {plugin.lastSeenAt ?? 'never'}
          </span>
        </div>
      </div>
      <button
        type="button"
        onClick={onRevoke}
        title="Revoke plugin"
        aria-label="revoke"
        className="p-2 rounded hover:bg-destructive/10 text-content-secondary hover:text-destructive transition-colors"
      >
        <Trash2 className="h-4 w-4" />
      </button>
    </li>
  )
}

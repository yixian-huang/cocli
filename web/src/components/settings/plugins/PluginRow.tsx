import type { Plugin } from '@shared/types'
export function PluginRow({ plugin }: { plugin: Plugin; onRevoke: () => void }) {
  return <li className="py-3 border-b last:border-b-0">{plugin.name}</li>
}

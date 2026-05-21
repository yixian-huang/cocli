import type { BlockProps } from './types'
import type { BlockActionBlockData } from './types'

export function BlockActionBlock({ data }: BlockProps) {
  const d = data as unknown as BlockActionBlockData

  return (
    <div className="flex items-center gap-2 px-3 py-2 bg-green-500/10 border-l-3 border-green-500 rounded-r-md text-sm">
      <span className="text-green-500">&#10003;</span>
      <span>
        {d.value || d.action_id}
        {d.form_data && (
          <span className="text-muted-foreground"> (form submitted)</span>
        )}
      </span>
    </div>
  )
}

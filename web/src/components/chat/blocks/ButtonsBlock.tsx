import { useState } from 'react'
import type { BlockProps } from './types'
import type { ButtonsBlockData } from './types'
import { messages } from '@/api/client'

const STYLE_CLASSES: Record<string, string> = {
  primary: 'bg-primary text-primary-foreground hover:bg-primary/90',
  success: 'bg-green-600 text-white hover:bg-green-700',
  danger: 'bg-red-600 text-white hover:bg-red-700',
  secondary: 'bg-accent text-accent-foreground hover:bg-accent/80',
}

export function ButtonsBlock({ data, messageId }: BlockProps) {
  const d = data as unknown as ButtonsBlockData
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const acted = !!d.acted_by

  const handleClick = async (actionId: string, value?: string) => {
    setLoading(true)
    setError(null)
    try {
      await messages.blockAction(messageId, { action_id: actionId, value })
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Action failed')
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="flex flex-col gap-2">
      {d.prompt && <div className="text-sm">{d.prompt}</div>}
      <div className="flex flex-wrap gap-2">
        {d.actions.map((action) => (
          <button
            key={action.id}
            disabled={acted || loading}
            onClick={() => handleClick(action.id, action.value)}
            className={`px-4 py-1.5 rounded-md text-sm font-medium transition-colors disabled:opacity-40 disabled:cursor-not-allowed ${STYLE_CLASSES[action.style || 'secondary']}`}
          >
            {action.label}
          </button>
        ))}
      </div>
      {acted && (
        <div className="text-xs text-green-500">
          &#10003; {d.acted_value || 'Acted'} by @{d.acted_by_name} &middot; {d.acted_at ? new Date(d.acted_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) : ''}
        </div>
      )}
      {error && <div className="text-xs text-red-500">{error}</div>}
    </div>
  )
}

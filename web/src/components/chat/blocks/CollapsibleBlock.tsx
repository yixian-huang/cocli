import { useState } from 'react'
import type { BlockProps } from './types'
import type { CollapsibleBlockData } from './types'
import { MarkdownRenderer } from '../MarkdownRenderer'

export function CollapsibleBlock({ data }: BlockProps) {
  const d = data as unknown as CollapsibleBlockData
  const [open, setOpen] = useState(d.default_open ?? false)

  return (
    <div className="rounded-lg bg-card overflow-hidden">
      <button
        onClick={() => setOpen(!open)}
        className="flex items-center gap-2 w-full px-3 py-2 text-sm hover:bg-accent/30 transition-colors"
      >
        <span className={`text-xs text-muted-foreground transition-transform ${open ? 'rotate-90' : ''}`}>
          &#9654;
        </span>
        <span>{d.title}</span>
      </button>
      {open && (
        <div className="px-3 pb-3 text-sm max-h-64 overflow-y-auto">
          {d.content_type === 'markdown' ? (
            <div className="prose prose-sm dark:prose-invert max-w-none">
              <MarkdownRenderer>{d.content}</MarkdownRenderer>
            </div>
          ) : (
            <pre className="whitespace-pre-wrap text-muted-foreground font-mono text-xs">{d.content}</pre>
          )}
        </div>
      )}
    </div>
  )
}

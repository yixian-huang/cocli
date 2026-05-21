import { useState } from 'react'
import type { BlockProps } from './types'
import type { TabsBlockData } from './types'
import { MarkdownRenderer } from '../MarkdownRenderer'

export function TabsBlock({ data }: BlockProps) {
  const d = data as unknown as TabsBlockData
  const defaultIdx = Math.max(0, Math.min(d.default_tab ?? 0, d.tabs.length - 1))
  const [active, setActive] = useState(defaultIdx)

  return (
    <div className="rounded-lg bg-card overflow-hidden">
      <div className="flex border-b border-border/30">
        {d.tabs.map((tab, i) => (
          <button
            key={i}
            onClick={() => setActive(i)}
            className={`px-4 py-2 text-xs transition-colors border-b-2 ${
              i === active
                ? 'border-primary text-primary'
                : 'border-transparent text-muted-foreground hover:text-foreground'
            }`}
          >
            {tab.label}
          </button>
        ))}
      </div>
      <div className="p-3 text-sm">
        {d.tabs[active]?.content_type === 'markdown' ? (
          <div className="prose prose-sm dark:prose-invert max-w-none">
            <MarkdownRenderer>{d.tabs[active].content}</MarkdownRenderer>
          </div>
        ) : (
          <pre className="whitespace-pre-wrap">{d.tabs[active]?.content}</pre>
        )}
      </div>
    </div>
  )
}

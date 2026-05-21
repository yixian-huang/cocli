import type { BlockProps } from './types'
import type { InfoCardBlockData } from './types'
import { MarkdownRenderer } from '../MarkdownRenderer'

const STATUS_STYLES: Record<string, { border: string; bg: string; text: string }> = {
  success: { border: 'border-green-500', bg: 'bg-green-500/10', text: 'text-green-500' },
  error: { border: 'border-red-500', bg: 'bg-red-500/10', text: 'text-red-500' },
  warning: { border: 'border-yellow-500', bg: 'bg-yellow-500/10', text: 'text-yellow-500' },
  info: { border: 'border-blue-500', bg: 'bg-blue-500/10', text: 'text-blue-500' },
}

export function InfoCardBlock({ data }: BlockProps) {
  const d = data as unknown as InfoCardBlockData
  const styles = STATUS_STYLES[d.status || 'info'] || STATUS_STYLES.info

  return (
    <div className={`rounded-lg bg-card overflow-hidden border-l-3 ${styles.border}`}>
      <div className="flex items-center gap-3 px-4 py-3">
        <div className="font-semibold text-sm">{d.title}</div>
        {d.status && (
          <span className={`text-xs px-2 py-0.5 rounded-full font-medium ${styles.bg} ${styles.text}`}>
            {d.status}
          </span>
        )}
      </div>
      {d.fields && d.fields.length > 0 && (
        <div className="grid grid-cols-2 gap-px bg-border/20">
          {d.fields.map((f, i) => (
            <div key={i} className="px-4 py-2 bg-card">
              <div className="text-xs text-muted-foreground">{f.label}</div>
              <div className="text-sm">{f.value}</div>
            </div>
          ))}
        </div>
      )}
      {d.description && (
        <div className="px-4 py-2 border-t border-border/20 text-sm text-muted-foreground prose prose-sm dark:prose-invert max-w-none">
          <MarkdownRenderer>{d.description}</MarkdownRenderer>
        </div>
      )}
    </div>
  )
}

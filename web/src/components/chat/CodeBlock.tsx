import { useCallback } from 'react'

interface Props {
  className?: string
  children?: React.ReactNode
}

export function CodeBlock({ className, children }: Props) {
  const match = /language-(\w+)/.exec(className || '')
  const language = match ? match[1] : ''
  const code = String(children).replace(/\n$/, '')

  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(code)
  }, [code])

  if (!className) {
    // Inline code
    return <code className="px-1.5 py-0.5 rounded bg-accent/60 text-sm">{children}</code>
  }

  return (
    <div className="relative rounded-[var(--radius-base)] overflow-hidden my-2 bg-surface-raised max-w-full">
      <div className="flex items-center justify-between px-3 py-1.5 bg-surface-panel text-xs">
        <span className="text-muted-foreground">{language || 'code'}</span>
        <button
          onClick={handleCopy}
          className="text-primary hover:text-primary/80 transition-colors"
        >
          Copy
        </button>
      </div>
      <pre className="p-3 overflow-x-auto max-w-full">
        <code className={className}>{children}</code>
      </pre>
    </div>
  )
}

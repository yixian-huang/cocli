import type { BlockProps } from './types'
import type { MarkdownBlockData } from './types'
import { MarkdownRenderer } from '../MarkdownRenderer'

export function MarkdownBlock({ data }: BlockProps) {
  const d = data as unknown as MarkdownBlockData
  return (
    <div className="prose prose-sm dark:prose-invert max-w-none">
      <MarkdownRenderer>{d.content}</MarkdownRenderer>
    </div>
  )
}

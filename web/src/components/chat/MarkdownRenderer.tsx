import { lazy, Suspense } from 'react'
import remarkGfm from 'remark-gfm'
import remarkMath from 'remark-math'
import rehypeHighlight from 'rehype-highlight'
import rehypeKatex from 'rehype-katex'
import { CodeBlock } from './CodeBlock'
import 'highlight.js/styles/github-dark.min.css'
import 'katex/dist/katex.min.css'

const ReactMarkdown = lazy(() => import('react-markdown'))

interface Props {
  children: string
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  components?: Record<string, React.ComponentType<any>>
}

export function MarkdownRenderer({ children, components }: Props) {
  return (
    <Suspense fallback={<span className="whitespace-pre-wrap">{children}</span>}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkMath]}
        rehypePlugins={[rehypeHighlight, rehypeKatex]}
        components={{
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          code: CodeBlock as unknown as React.ComponentType<any>,
          ...components,
        }}
      >
        {children}
      </ReactMarkdown>
    </Suspense>
  )
}

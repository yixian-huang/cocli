import { useState } from 'react'
import type { TrajectoryEntry } from '@/lib/types'
import { Badge } from '@/components/ui'
import { parseContextAutoForkDetail, contextAutoForkModeVariant } from '@/lib/contextAutoForkDetail'
import {
  ChevronDown,
  ChevronRight,
  Brain,
  FileText,
  Wrench,
  AlertCircle,
  MessageSquare,
} from 'lucide-react'

function ThinkingEntry({ entry }: { entry: TrajectoryEntry }) {
  const [open, setOpen] = useState(false)
  return (
    <div className="flex items-start gap-2 py-1">
      <Brain className="h-3.5 w-3.5 mt-0.5 shrink-0 text-primary" />
      <div className="flex-1 min-w-0">
        <button
          onClick={() => setOpen((v) => !v)}
          className="flex items-center gap-1 text-[11px] text-primary font-medium hover:text-primary/80"
        >
          {open ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />}
          Thinking
        </button>
        {open && entry.text && (
          <p className="mt-1 text-[11px] text-muted-foreground italic whitespace-pre-wrap leading-relaxed pl-1 border-l-2 border-primary/20 dark:border-primary/30">
            {entry.text}
          </p>
        )}
      </div>
    </div>
  )
}

function isSimpleValue(v: unknown): boolean {
  return (typeof v === 'string' || typeof v === 'number' || typeof v === 'boolean') && String(v).length <= 80
}

function StructuredKV({ data }: { data: Record<string, unknown> }) {
  return (
    <div className="mt-1 space-y-0.5">
      {Object.entries(data).map(([key, value]) => (
        <div key={key} className="flex gap-1.5 text-[11px] leading-relaxed">
          <span className="font-mono text-muted-foreground shrink-0">{key}:</span>
          {typeof value === 'string' && value.length > 120 ? (
            <span className="text-foreground break-all whitespace-pre-wrap">{value}</span>
          ) : typeof value === 'object' && value !== null ? (
            <pre className="text-[10px] bg-muted/50 rounded px-1.5 py-0.5 overflow-x-auto max-w-full whitespace-pre-wrap break-all">
              {JSON.stringify(value, null, 2)}
            </pre>
          ) : (
            <span className="text-foreground break-all">{String(value)}</span>
          )}
        </div>
      ))}
    </div>
  )
}

function ToolCallEntry({ entry }: { entry: TrajectoryEntry }) {
  const [open, setOpen] = useState(false)
  const toolName =
    entry.input && typeof (entry.input as { name?: string }).name === 'string'
      ? (entry.input as { name: string }).name
      : 'tool'
  const inputArgs = entry.input ? { ...entry.input } : {}
  delete (inputArgs as Record<string, unknown>).name

  const keys = Object.keys(inputArgs)
  const isSimple = keys.length > 0 && keys.length <= 3 && keys.every(k => isSimpleValue((inputArgs as Record<string, unknown>)[k]))

  if (isSimple) {
    return (
      <div className="flex items-start gap-2 py-1">
        <Wrench className="mt-0.5 h-3.5 w-3.5 shrink-0 text-info-emphasis" />
        <div className="flex-1 min-w-0">
          <div className="flex items-baseline gap-1.5 flex-wrap text-[11px]">
            <span className="font-mono font-medium text-info-emphasis">{toolName}</span>
            {keys.map((k) => (
              <span key={k} className="text-muted-foreground">
                <span className="font-mono">{k}</span>=<span className="text-foreground">{String((inputArgs as Record<string, unknown>)[k])}</span>
              </span>
            ))}
          </div>
        </div>
      </div>
    )
  }

  return (
    <div className="flex items-start gap-2 py-1">
      <Wrench className="mt-0.5 h-3.5 w-3.5 shrink-0 text-info-emphasis" />
      <div className="flex-1 min-w-0">
        <button
          onClick={() => setOpen((v) => !v)}
          className="flex items-center gap-1 text-[11px] font-medium text-info-emphasis hover:text-info-emphasis/80"
        >
          {open ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />}
          <span className="font-mono">{toolName}</span>
          {!open && keys.length > 0 && (
            <span className="text-muted-foreground font-normal">({keys.length} params)</span>
          )}
        </button>
        {open && keys.length > 0 && (
          <StructuredKV data={inputArgs as Record<string, unknown>} />
        )}
      </div>
    </div>
  )
}

function tryParseJSON(str: string): Record<string, unknown> | null {
  try {
    const parsed = JSON.parse(str)
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) return parsed
  } catch { /* not JSON */ }
  return null
}

function ToolResultEntry({ entry }: { entry: TrajectoryEntry }) {
  const [open, setOpen] = useState(false)
  // When the runtime reported a tool failure, `error` is set instead of (or
  // alongside) `result`. Render in error styling and label "Error" so admin
  // can tell failure from success at a glance.
  const isError = !!entry.error
  const body = isError ? entry.error || '' : entry.result || ''
  const label = isError ? 'Error' : 'Result'
  const Icon = isError ? AlertCircle : FileText
  const tone = isError ? 'text-error-emphasis' : 'text-success-emphasis'
  const isShort = body.length <= 120
  const parsed = tryParseJSON(body)

  if (isShort && !parsed) {
    return (
      <div className="flex items-start gap-2 py-1">
        <Icon className={`mt-0.5 h-3.5 w-3.5 shrink-0 ${tone}`} />
        <div className="flex-1 min-w-0">
          <div className="text-[11px]">
            <span className={`font-medium ${tone}`}>{label}: </span>
            <span className="text-foreground">{body}</span>
          </div>
        </div>
      </div>
    )
  }

  return (
    <div className="flex items-start gap-2 py-1">
      <Icon className={`mt-0.5 h-3.5 w-3.5 shrink-0 ${tone}`} />
      <div className="flex-1 min-w-0">
        <button
          onClick={() => setOpen((v) => !v)}
          className={`flex items-center gap-1 text-[11px] font-medium ${tone} hover:opacity-80`}
        >
          {open ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />}
          {label}
          {!open && (
            <span className="text-muted-foreground font-normal truncate max-w-[200px]">
              — {parsed ? `{${Object.keys(parsed).length} fields}` : `${body.length} chars`}
            </span>
          )}
        </button>
        {open && (
          parsed ? (
            <StructuredKV data={parsed} />
          ) : (
            <pre className="mt-1 text-[10px] bg-muted rounded p-2 overflow-x-auto max-w-full leading-relaxed whitespace-pre-wrap">
              {body}
            </pre>
          )
        )}
      </div>
    </div>
  )
}

function InputEntry({ entry }: { entry: TrajectoryEntry }) {
  const [open, setOpen] = useState(false)
  return (
    <div className="flex items-start gap-2 py-1">
      <MessageSquare className="mt-0.5 h-3.5 w-3.5 shrink-0 text-accent-primary" />
      <div className="flex-1 min-w-0">
        <button
          onClick={() => setOpen((v) => !v)}
          className="flex items-center gap-1 text-[11px] font-medium text-accent-primary hover:text-accent-primary/80"
        >
          {open ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />}
          Input
        </button>
        {open && entry.text && (
          <p className="mt-1 border-l-2 border-accent-primary/20 pl-1 text-[11px] leading-relaxed whitespace-pre-wrap text-muted-foreground">
            {entry.text}
          </p>
        )}
      </div>
    </div>
  )
}

function TextEntry({ entry }: { entry: TrajectoryEntry }) {
  return (
    <div className="py-1 text-[12px] leading-relaxed text-foreground whitespace-pre-wrap">
      {entry.text}
    </div>
  )
}

function StatusEntry({ entry }: { entry: TrajectoryEntry }) {
  const parsed = parseContextAutoForkDetail(entry.text)
  if (!parsed.text) return null
  return (
    <div className="py-0.5 text-[11px] text-muted-foreground italic flex flex-wrap items-center gap-1.5">
      <span>{parsed.text}</span>
      {parsed.mode && (
        <Badge
          variant={contextAutoForkModeVariant(parsed.mode)}
          size="sm"
          className="not-italic normal-case"
        >
          {parsed.mode}
        </Badge>
      )}
    </div>
  )
}

function ErrorEntry({ entry }: { entry: TrajectoryEntry }) {
  return (
    <div className="flex items-start gap-2 py-1">
      <AlertCircle className="mt-0.5 h-3.5 w-3.5 shrink-0 text-error-emphasis" />
      <span className="text-[11px] text-error-emphasis">{entry.text}</span>
    </div>
  )
}

export function renderEntry(entry: TrajectoryEntry): React.ReactNode {
  switch (entry.kind) {
    case 'input':       return <InputEntry entry={entry} />
    case 'thinking':    return <ThinkingEntry entry={entry} />
    case 'tool_call':   return <ToolCallEntry entry={entry} />
    case 'tool_result': return <ToolResultEntry entry={entry} />
    case 'text':        return <TextEntry entry={entry} />
    case 'status':      return <StatusEntry entry={entry} />
    case 'error':       return <ErrorEntry entry={entry} />
    default:            return null
  }
}

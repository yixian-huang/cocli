import { useEffect, useRef, useState } from 'react'
import { Search, X } from 'lucide-react'
import { cn } from '@/lib/utils'

interface Props {
  value: string
  onChange: (next: string) => void
  placeholder?: string
  resultCount?: number
  totalCount?: number
  shortcut?: string
  className?: string
}

const DEBOUNCE_MS = 120

export function ListFilter({
  value,
  onChange,
  placeholder = 'Filter…',
  resultCount,
  totalCount,
  shortcut,
  className,
}: Props) {
  const [local, setLocal] = useState(value)
  const inputRef = useRef<HTMLInputElement>(null)
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  useEffect(() => {
    setLocal(value)
  }, [value])

  useEffect(() => {
    if (!shortcut) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === shortcut && document.activeElement?.tagName !== 'INPUT') {
        e.preventDefault()
        inputRef.current?.focus()
      }
    }
    document.addEventListener('keydown', onKey)
    return () => document.removeEventListener('keydown', onKey)
  }, [shortcut])

  const flush = (next: string) => {
    if (timerRef.current) clearTimeout(timerRef.current)
    timerRef.current = setTimeout(() => onChange(next), DEBOUNCE_MS)
  }

  return (
    <div className={cn('relative px-2 pb-1', className)}>
      <Search className="pointer-events-none absolute left-4 top-1/2 -translate-y-1/2 h-3 w-3 text-muted-foreground" />
      <input
        ref={inputRef}
        role="searchbox"
        type="text"
        value={local}
        placeholder={placeholder}
        className="w-full pl-7 pr-7 py-1 text-xs rounded bg-input/40 border border-border focus:outline-none focus:ring-1 focus:ring-ring"
        onChange={(e) => {
          setLocal(e.target.value)
          flush(e.target.value)
        }}
        onKeyDown={(e) => {
          if (e.key === 'Escape') {
            setLocal('')
            flush('')
            ;(e.target as HTMLInputElement).blur()
          }
        }}
      />
      {local && (
        <button
          type="button"
          onClick={() => {
            setLocal('')
            flush('')
          }}
          className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
          aria-label="Clear filter"
        >
          <X className="h-3 w-3" />
        </button>
      )}
      {typeof resultCount === 'number' && typeof totalCount === 'number' && (
        <div className="px-1 pt-0.5 text-[10px] text-muted-foreground">
          {resultCount} of {totalCount} match
        </div>
      )}
    </div>
  )
}

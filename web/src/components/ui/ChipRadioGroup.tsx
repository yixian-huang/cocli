import { useId, useRef, type HTMLAttributes, type KeyboardEvent, type ReactNode } from 'react'
import { cn } from '@/lib/utils'
import { Tooltip } from './Tooltip'

export type ChipRadioTone = 'neutral' | 'info' | 'warn' | 'danger'

export interface ChipRadioOption<T extends string> {
  value: T
  label: string
  description?: string
  icon?: ReactNode
  tone?: ChipRadioTone
  disabled?: boolean
}

interface ChipRadioGroupProps<T extends string> extends Omit<HTMLAttributes<HTMLDivElement>, 'onChange'> {
  value: T
  onChange: (value: T) => void
  options: readonly ChipRadioOption<T>[]
  disabled?: boolean
}

const selectedToneClasses: Record<ChipRadioTone, string> = {
  neutral: 'border-foreground/10 bg-foreground/[0.06] text-foreground ring-2 ring-ring/20 ring-offset-1 ring-offset-background',
  info: 'border-sky-500/40 bg-sky-500/12 text-sky-700 ring-2 ring-sky-500/30 ring-offset-1 ring-offset-background dark:text-sky-300',
  warn: 'border-amber-500/40 bg-amber-500/15 text-amber-700 ring-2 ring-amber-500/30 ring-offset-1 ring-offset-background dark:text-amber-300',
  danger: 'border-red-500/50 bg-red-500/12 text-red-700 ring-2 ring-red-500/35 ring-offset-1 ring-offset-background dark:text-red-300',
}

const unselectedClasses = 'border-border/70 bg-muted/40 text-muted-foreground hover:border-border hover:bg-accent hover:text-accent-foreground'

export function ChipRadioGroup<T extends string>({
  value,
  onChange,
  options,
  disabled = false,
  className,
  id,
  ...props
}: ChipRadioGroupProps<T>) {
  const autoId = useId()
  const groupId = id ?? autoId
  const chipRefs = useRef<Array<HTMLButtonElement | null>>([])
  const selectedIndex = options.findIndex((option) => option.value === value)
  const firstEnabledIndex = options.findIndex((option) => !option.disabled)

  const focusOption = (index: number) => {
    chipRefs.current[index]?.focus()
  }

  const commitSelection = (index: number) => {
    const option = options[index]
    if (!option || disabled || option.disabled) return
    onChange(option.value)
    focusOption(index)
  }

  const moveSelection = (startIndex: number, direction: 1 | -1) => {
    if (options.length === 0) return
    let nextIndex = startIndex
    for (let i = 0; i < options.length; i += 1) {
      nextIndex = (nextIndex + direction + options.length) % options.length
      if (!options[nextIndex]?.disabled) {
        commitSelection(nextIndex)
        return
      }
    }
  }

  const findBoundaryIndex = (direction: 'start' | 'end') => {
    const candidates = direction === 'start' ? options : [...options].reverse()
    const found = candidates.find((option) => !option.disabled)
    if (!found) return -1
    return options.findIndex((option) => option.value === found.value)
  }

  const handleKeyDown = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    switch (event.key) {
      case 'ArrowRight':
      case 'ArrowDown':
        event.preventDefault()
        moveSelection(index, 1)
        break
      case 'ArrowLeft':
      case 'ArrowUp':
        event.preventDefault()
        moveSelection(index, -1)
        break
      case 'Home': {
        event.preventDefault()
        const boundaryIndex = findBoundaryIndex('start')
        if (boundaryIndex >= 0) commitSelection(boundaryIndex)
        break
      }
      case 'End': {
        event.preventDefault()
        const boundaryIndex = findBoundaryIndex('end')
        if (boundaryIndex >= 0) commitSelection(boundaryIndex)
        break
      }
      case ' ':
      case 'Enter':
        event.preventDefault()
        commitSelection(index)
        break
      default:
        break
    }
  }

  return (
    <div
      {...props}
      id={groupId}
      role="radiogroup"
      className={cn('flex flex-wrap items-center gap-1.5', className)}
    >
      {options.map((option, index) => {
        const isSelected = option.value === value
        const tone = option.tone ?? 'neutral'
        const button = (
          <button
            key={option.value}
            ref={(node) => {
              chipRefs.current[index] = node
            }}
            type="button"
            role="radio"
            aria-checked={isSelected}
            aria-disabled={disabled || option.disabled || undefined}
            data-state={isSelected ? 'checked' : 'unchecked'}
            data-tone={tone}
            disabled={disabled || option.disabled}
            tabIndex={isSelected || (selectedIndex === -1 && index === firstEnabledIndex) ? 0 : -1}
            title={option.description}
            onClick={() => commitSelection(index)}
            onKeyDown={(event) => handleKeyDown(event, index)}
            className={cn(
              'inline-flex h-8 items-center gap-1.5 rounded-full border px-2.5 text-xs font-medium shadow-sm transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50',
              isSelected ? selectedToneClasses[tone] : unselectedClasses,
            )}
          >
            {option.icon ? <span className="shrink-0" aria-hidden>{option.icon}</span> : null}
            <span>{option.label}</span>
          </button>
        )

        if (option.description) {
          return (
            <Tooltip key={option.value} content={option.description} delay={150}>
              {button}
            </Tooltip>
          )
        }

        return button
      })}
    </div>
  )
}

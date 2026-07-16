import {
  useEffect,
  useId,
  useRef,
  useState,
  type KeyboardEvent,
} from 'react'
import { Check, ChevronDown } from 'lucide-react'

export interface LocalSelectOption {
  value: string
  label: string
  meta?: string
}

interface LocalSelectProps {
  id: string
  ariaLabel: string
  value: string
  options: readonly LocalSelectOption[]
  onChange: (value: string) => void
  disabled?: boolean
  placeholder: string
  compact?: boolean
}

export function LocalSelect({
  id,
  ariaLabel,
  value,
  options,
  onChange,
  disabled = false,
  placeholder,
  compact = false,
}: LocalSelectProps) {
  const [open, setOpen] = useState(false)
  const [activeIndex, setActiveIndex] = useState(0)
  const rootRef = useRef<HTMLDivElement>(null)
  const listboxId = useId()
  const selectedIndex = Math.max(0, options.findIndex((option) => option.value === value))
  const selected = options[selectedIndex]

  useEffect(() => {
    if (!open) return
    setActiveIndex(selectedIndex)
    const closeOnOutsideClick = (event: MouseEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false)
    }
    document.addEventListener('mousedown', closeOnOutsideClick)
    return () => document.removeEventListener('mousedown', closeOnOutsideClick)
  }, [open, selectedIndex])

  function choose(index: number) {
    const option = options[index]
    if (!option) return
    onChange(option.value)
    setOpen(false)
  }

  function handleKeyDown(event: KeyboardEvent<HTMLButtonElement>) {
    if (disabled || options.length === 0) return
    if (event.key === 'Escape') {
      setOpen(false)
      return
    }
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault()
      setOpen(true)
      setActiveIndex((current) => {
        const direction = event.key === 'ArrowDown' ? 1 : -1
        return (current + direction + options.length) % options.length
      })
      return
    }
    if (event.key === 'Home' || event.key === 'End') {
      event.preventDefault()
      setOpen(true)
      setActiveIndex(event.key === 'Home' ? 0 : options.length - 1)
      return
    }
    if ((event.key === 'Enter' || event.key === ' ') && open) {
      event.preventDefault()
      choose(activeIndex)
    }
  }

  return (
    <div
      ref={rootRef}
      className={`local-select${compact ? ' compact' : ''}${open ? ' open' : ''}`}
    >
      <button
        id={id}
        type="button"
        className="local-select-trigger"
        aria-label={ariaLabel}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={listboxId}
        disabled={disabled}
        onClick={() => setOpen((current) => !current)}
        onKeyDown={handleKeyDown}
      >
        <span className={!selected ? 'placeholder' : ''}>
          {selected?.label ?? placeholder}
        </span>
        {selected?.meta && <small>{selected.meta}</small>}
        <ChevronDown size={15} strokeWidth={1.8} aria-hidden="true" />
      </button>

      {open && (
        <div
          id={listboxId}
          className="local-select-popover"
          role="listbox"
          aria-label={ariaLabel}
          aria-activedescendant={`${listboxId}-${activeIndex}`}
        >
          {options.map((option, index) => (
            <button
              id={`${listboxId}-${index}`}
              key={option.value}
              type="button"
              role="option"
              aria-selected={option.value === value}
              className={index === activeIndex ? 'active' : ''}
              onMouseEnter={() => setActiveIndex(index)}
              onClick={() => choose(index)}
            >
              <span>
                <strong>{option.label}</strong>
                {option.meta && <small>{option.meta}</small>}
              </span>
              {option.value === value && <Check size={15} strokeWidth={2} aria-hidden="true" />}
            </button>
          ))}
        </div>
      )}
    </div>
  )
}

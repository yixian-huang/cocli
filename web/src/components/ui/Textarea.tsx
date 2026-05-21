import { forwardRef, useCallback } from 'react'
import { cn } from '@/lib/utils'

interface TextareaProps extends React.TextareaHTMLAttributes<HTMLTextAreaElement> {
  label?: string
  error?: string
}

const Textarea = forwardRef<HTMLTextAreaElement, TextareaProps>(
  ({ className, label, error, onChange, ...props }, ref) => {
    const handleChange = useCallback((e: React.ChangeEvent<HTMLTextAreaElement>) => {
      const el = e.currentTarget
      el.style.height = 'auto'
      el.style.height = `${el.scrollHeight}px`
      onChange?.(e)
    }, [onChange])
    return (
      <div className="w-full">
        {label && <label className="block text-sm font-medium text-foreground mb-1.5">{label}</label>}
        <textarea
          ref={ref} onChange={handleChange}
          className={cn(
            'flex min-h-[88px] w-full rounded-md border bg-background px-3 py-2.5 text-base transition-colors resize-none',
            'placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
            'disabled:cursor-not-allowed disabled:opacity-50',
            error ? 'border-destructive' : 'border-input', className
          )}
          {...props}
        />
        {error && <p className="mt-1.5 text-sm text-destructive">{error}</p>}
      </div>
    )
  }
)
Textarea.displayName = 'Textarea'
export { Textarea }

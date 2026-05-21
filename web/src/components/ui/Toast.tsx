import { useEffect } from 'react'
import { AlertCircle, AlertTriangle, CheckCircle, Info, X } from 'lucide-react'
import { cn } from '@/lib/utils'
import { useToastStore, type Toast as ToastRecord } from '@/stores/toastStore'

const iconMap: Record<ToastRecord['type'], typeof CheckCircle> = {
  success: CheckCircle,
  info: Info,
  warn: AlertTriangle,
  error: AlertCircle,
  critical: AlertCircle,
}

const toneClasses: Record<ToastRecord['type'], string> = {
  success: 'border-success/25 bg-success/10 text-success',
  info: 'border-info/25 bg-info/10 text-info',
  warn: 'border-warning/25 bg-warning/10 text-warning',
  error: 'border-error/30 bg-error/10 text-error',
  critical: 'border-error/40 bg-error/15 text-error shadow-lg shadow-error/10',
}

const phaseClasses: Record<ToastRecord['phase'], string> = {
  entering: 'translate-x-8 opacity-0',
  visible: 'translate-x-0 opacity-100',
  closing: 'translate-x-4 opacity-0',
}

function isAssertive(type: ToastRecord['type']) {
  return type === 'warn' || type === 'error' || type === 'critical'
}

export function ToastContainer() {
  const toasts = useToastStore((state) => state.toasts)
  const dismissToast = useToastStore((state) => state.dismissToast)
  const dismissLatest = useToastStore((state) => state.dismissLatestToast)

  useEffect(() => {
    if (!toasts.length) return

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape' || event.defaultPrevented) return
      event.preventDefault()
      dismissLatest()
    }

    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [dismissLatest, toasts.length])

  if (!toasts.length) {
    return null
  }

  return (
    <div className="pointer-events-none fixed bottom-4 right-4 z-50 flex max-w-sm flex-col gap-2" aria-live="polite">
      {toasts.map((toast) => {
        const Icon = iconMap[toast.type]
        const assertive = isAssertive(toast.type)

        return (
          <div
            key={toast.id}
            role={assertive ? 'alert' : 'status'}
            aria-live={assertive ? 'assertive' : 'polite'}
            className={cn(
              'pointer-events-auto relative flex items-start gap-3 rounded-xl border px-4 py-3 shadow-whisper backdrop-blur-sm transition-all duration-200 ease-out',
              toneClasses[toast.type],
              phaseClasses[toast.phase],
            )}
          >
            <span className={cn('mt-0.5 shrink-0', toast.type === 'critical' && 'animate-pulse')}>
              <Icon className="h-4 w-4" />
            </span>
            <div className="min-w-0 flex-1 text-sm font-medium leading-5">
              <p className="break-words">{toast.message}</p>
            </div>
            <button
              type="button"
              onClick={() => dismissToast(toast.id)}
              className="shrink-0 rounded-md p-1 opacity-70 transition-opacity hover:opacity-100 focus:outline-none focus:ring-2 focus:ring-current/30"
              aria-label={`Dismiss ${toast.type} notification`}
            >
              <X className="h-3.5 w-3.5" />
            </button>
          </div>
        )
      })}
    </div>
  )
}

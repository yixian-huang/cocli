import { AnimatePresence, motion } from 'framer-motion'
import { X } from 'lucide-react'
import { useKeyboardShortcuts } from '@/hooks/useKeyboardShortcuts'
import { cn } from '@/lib/utils'

interface ModalProps {
  open: boolean
  onClose: () => void
  title?: string
  children: React.ReactNode
  footer?: React.ReactNode
  size?: 'sm' | 'md' | 'lg'
  className?: string
}

const sizeClasses = { sm: 'max-w-[400px]', md: 'max-w-[500px]', lg: 'max-w-[640px]' }

function Modal({ open, onClose, title, children, footer, size = 'md', className }: ModalProps) {
  useKeyboardShortcuts([
    {
      key: 'escape',
      enabled: open,
      allowInInput: true,
      priority: 100,
      handler: onClose,
    },
  ])

  return (
    <AnimatePresence>
      {open && (
        <div className="fixed inset-0 z-[60] flex items-center justify-center">
          <motion.div className="absolute inset-0 bg-[color:color-mix(in_srgb,#000_60%,transparent)]" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} onClick={onClose} />
          <motion.div
            className={cn('relative w-full mx-4 rounded-lg border border-border bg-background shadow-elev-hover', sizeClasses[size], className)}
            initial={{ opacity: 0, scale: 0.95 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={{ opacity: 0, scale: 0.95 }}
            transition={{ type: 'spring', stiffness: 300, damping: 30 }}
          >
            {title && (
              <div className="flex items-center justify-between px-4 py-3 border-b border-border">
                <h3 className="text-sm font-medium text-foreground">{title}</h3>
                <button
                  onClick={onClose}
                  aria-label="Close modal"
                  className={cn(
                    'text-muted-foreground rounded-md p-1',
                    'transition-[color,transform] duration-[var(--motion-base)] ease-[var(--motion-fn)]',
                    'hover:text-accent-signature hover:scale-110',
                    'active:scale-95 active:duration-[var(--motion-fast)]',
                    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
                  )}
                >
                  <X className="h-4 w-4" />
                </button>
              </div>
            )}
            <div className="p-4">{children}</div>
            {footer && <div className="flex items-center justify-end gap-2 px-4 py-3 border-t border-border">{footer}</div>}
          </motion.div>
        </div>
      )}
    </AnimatePresence>
  )
}

export { Modal }

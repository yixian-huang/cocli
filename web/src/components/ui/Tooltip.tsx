import { useState, useRef } from 'react'
import { cn } from '@/lib/utils'

interface TooltipProps {
  content: string
  children: React.ReactNode
  side?: 'top' | 'bottom' | 'left' | 'right'
  delay?: number
}

const positionClasses = {
  top: 'bottom-full left-1/2 -translate-x-1/2 mb-2',
  bottom: 'top-full left-1/2 -translate-x-1/2 mt-2',
  left: 'right-full top-1/2 -translate-y-1/2 mr-2',
  right: 'left-full top-1/2 -translate-y-1/2 ml-2',
}

const arrowClasses = {
  top: 'top-full left-1/2 -translate-x-1/2 border-t-popover border-x-transparent border-b-transparent',
  bottom: 'bottom-full left-1/2 -translate-x-1/2 border-b-popover border-x-transparent border-t-transparent',
  left: 'left-full top-1/2 -translate-y-1/2 border-l-popover border-y-transparent border-r-transparent',
  right: 'right-full top-1/2 -translate-y-1/2 border-r-popover border-y-transparent border-l-transparent',
}

function Tooltip({ content, children, side = 'top', delay = 500 }: TooltipProps) {
  const [visible, setVisible] = useState(false)
  const timerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)

  const show = () => { timerRef.current = setTimeout(() => setVisible(true), delay) }
  const hide = () => { clearTimeout(timerRef.current); setVisible(false) }

  return (
    <div className="relative inline-flex" onMouseEnter={show} onMouseLeave={hide}>
      {children}
      <div
        role="tooltip"
        data-state={visible ? 'open' : 'closed'}
        className={cn(
          'absolute z-50 pointer-events-none',
          positionClasses[side],
          'transition-[opacity,transform] ease-[var(--motion-fn)]',
          'data-[state=open]:opacity-100 data-[state=open]:scale-100 data-[state=open]:duration-[var(--motion-fast)]',
          'data-[state=closed]:opacity-0 data-[state=closed]:scale-[0.94] data-[state=closed]:duration-[80ms]',
        )}
      >
        <div className="bg-popover text-popover-foreground text-xs px-2 py-1.5 rounded-md shadow-elev-hover border border-border max-w-[260px] whitespace-normal leading-snug">
          {content}
        </div>
        <div className={cn('absolute border-4', arrowClasses[side])} />
      </div>
    </div>
  )
}

export { Tooltip }

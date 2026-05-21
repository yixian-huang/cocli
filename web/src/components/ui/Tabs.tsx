import { useId } from 'react'
import { motion } from 'framer-motion'
import { cn } from '@/lib/utils'

interface TabsProps {
  tabs: { key: string; label: React.ReactNode; icon?: React.ReactNode }[]
  active: string
  onChange: (key: string) => void
  size?: 'sm' | 'md'
  className?: string
}

const sizeClasses = { sm: 'text-sm py-2', md: 'text-base py-2.5' }

function Tabs({ tabs, active, onChange, size = 'md', className }: TabsProps) {
  const instanceId = useId()
  const layoutId = `tabs-active-bar-${instanceId}`

  return (
    <div role="tablist" className={cn('flex border-b border-border', className)}>
      {tabs.map((tab) => {
        const isActive = tab.key === active
        return (
          <button
            key={tab.key}
            role="tab"
            aria-selected={isActive}
            onClick={() => onChange(tab.key)}
            className={cn(
              'relative flex-1 flex items-center justify-center gap-1.5 font-medium transition-colors duration-[var(--motion-base)] ease-[var(--motion-fn)]',
              sizeClasses[size],
              isActive ? 'text-primary' : 'text-content-secondary hover:text-foreground',
            )}
          >
            {tab.icon}
            {tab.label}
            {isActive && (
              <motion.div
                layoutId={layoutId}
                data-tab-active-bar
                className="absolute bottom-[-1px] left-0 right-0 h-[2px] bg-primary"
                transition={{ type: 'spring', stiffness: 400, damping: 35 }}
              />
            )}
          </button>
        )
      })}
    </div>
  )
}

export { Tabs }

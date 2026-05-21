import { cn, avatarColor, avatarInitial } from '@/lib/utils'
import { StatusDot } from './StatusDot'

interface AvatarProps {
  name: string
  size?: 'sm' | 'md' | 'lg'
  image?: string
  status?: 'online' | 'offline' | 'working' | 'error'
  isBot?: boolean
  className?: string
}

const sizeClasses = { sm: 'w-6 h-6 text-[10px]', md: 'w-8 h-8 text-xs', lg: 'w-10 h-10 text-sm' }
const dotPositions = { sm: '-bottom-0.5 -right-0.5', md: '-bottom-0.5 -right-0.5', lg: 'bottom-0 right-0' }

function Avatar({ name, size = 'md', image, status, isBot, className }: AvatarProps) {
  const [bgClass, textClass] = avatarColor(name)
  const initial = avatarInitial(name)

  const shape = isBot ? 'rounded-md' : 'rounded-full'

  return (
    <div className={cn('relative inline-flex shrink-0', className)}>
      <div className={cn('flex items-center justify-center font-medium', shape, sizeClasses[size], image ? '' : `${bgClass} ${textClass}`)}>
        {image ? (
          <img src={image} alt={name} className={cn('w-full h-full object-cover', shape)} />
        ) : (
          initial
        )}
      </div>
      {status && (
        <span className={cn('absolute', dotPositions[size])}>
          <StatusDot status={status} size="sm" />
        </span>
      )}
    </div>
  )
}

export { Avatar }

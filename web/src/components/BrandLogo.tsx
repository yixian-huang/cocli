import { BRAND } from '@/brand'
import cocliIconUrl from '@/assets/brand/cocli-mark.svg'
import { cn } from '@/lib/utils'

interface BrandLogoProps {
  className?: string
  iconClassName?: string
  textClassName?: string
  showText?: boolean
}

export function BrandLogo({
  className,
  iconClassName,
  textClassName,
  showText = true,
}: BrandLogoProps) {
  return (
    <div className={cn('inline-flex items-center gap-2', className)}>
      <img
        src={cocliIconUrl}
        alt=""
        aria-hidden="true"
        className={cn('h-7 w-7 shrink-0 object-contain', iconClassName)}
      />
      {showText && (
        <span className={cn('font-serif font-medium text-foreground', textClassName)}>
          {BRAND.displayName}
        </span>
      )}
    </div>
  )
}

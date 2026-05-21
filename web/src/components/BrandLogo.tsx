import { cn } from '@/lib/utils'

type Size = 'sm' | 'md' | 'lg'

const sizeMap: Record<Size, string> = {
  sm: 'text-sm',
  md: 'text-base',
  lg: 'text-lg',
}

export function BrandLogo({
  size = 'md',
  textClassName,
}: {
  size?: Size
  textClassName?: string
}) {
  return (
    <span
      className={cn(
        'font-sans font-medium tracking-tight text-foreground select-none',
        sizeMap[size],
        textClassName,
      )}
    >
      cocli
    </span>
  )
}

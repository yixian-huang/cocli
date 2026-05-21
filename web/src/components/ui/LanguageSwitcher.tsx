import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { ChevronDown } from 'lucide-react'

import { cn } from '@/lib/utils'
import { Dropdown } from './Dropdown'

type LangOption = { value: string; label: string; flag: string }

const LANGS: LangOption[] = [
  { value: 'zh-CN', label: '简体中文', flag: '🇨🇳' },
  { value: 'en', label: 'English', flag: '🇺🇸' },
  { value: 'es', label: 'Español', flag: '🇪🇸' },
  { value: 'fr', label: 'Français', flag: '🇫🇷' },
  { value: 'de', label: 'Deutsch', flag: '🇩🇪' },
  { value: 'ja', label: '日本語', flag: '🇯🇵' },
]

export function LanguageSwitcher({ className, compact }: { className?: string; compact?: boolean }) {
  const { i18n } = useTranslation()

  const current = useMemo(() => {
    const value = i18n.resolvedLanguage || i18n.language
    return LANGS.find((l) => l.value === value) || LANGS.find((l) => value?.startsWith(l.value)) || LANGS[1]
  }, [i18n.language, i18n.resolvedLanguage])

  const items = useMemo(() => {
    return LANGS.map((l) => ({
      id: l.value,
      label: l.label,
      icon: (
        <span className="inline-flex w-5 items-center justify-center" aria-hidden="true">
          {l.flag}
        </span>
      ),
      onSelect: () => void i18n.changeLanguage(l.value),
    }))
  }, [i18n])

  const trigger = (
    <button
      type="button"
      className={cn(
        'inline-flex items-center gap-2 rounded-md border bg-background px-2.5',
        'text-xs text-foreground shadow-sm transition-colors hover:bg-accent',
        'focus:outline-none focus:ring-2 focus:ring-ring/30',
        compact ? 'h-7' : 'h-8',
        className,
      )}
      aria-label="Language"
    >
      <span className="inline-flex w-5 items-center justify-center" aria-hidden="true">
        {current.flag}
      </span>
      <span className={cn('max-w-[96px] truncate', compact && 'hidden sm:inline')}>
        {current.label}
      </span>
      <ChevronDown className="h-3.5 w-3.5 text-muted-foreground" aria-hidden="true" />
    </button>
  )

  return (
    <Dropdown
      trigger={trigger}
      items={items}
      align="right"
      // 只向下拉：Dropdown 默认用 absolute + mt-1；这里仅保证不会向上做翻转
      className="origin-top-right"
    />
  )
}


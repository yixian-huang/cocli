import { Keyboard } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Modal } from './Modal'

export interface ShortcutSection {
  title: string
  items: Array<{
    keys: string[]
    description: string
  }>
}

interface ShortcutsOverlayProps {
  open: boolean
  onClose: () => void
  sections: ShortcutSection[]
}

export function ShortcutsOverlay({ open, onClose, sections }: ShortcutsOverlayProps) {
  const { t } = useTranslation()
  return (
    <Modal open={open} onClose={onClose} title={t('workspace.shortcuts.title')} size="md">
      <div className="space-y-4" data-testid="shortcuts-overlay">
        {sections.map((section) => (
          <section key={section.title} className="space-y-2">
            <div className="flex items-center gap-2 text-sm font-semibold uppercase tracking-[0.12em] text-content-secondary">
              <Keyboard className="h-3.5 w-3.5" />
              <span>{section.title}</span>
            </div>
            <div className="space-y-2 rounded-lg border border-border/70 bg-accent/20 p-3">
              {section.items.map((item) => (
                <div key={`${section.title}-${item.description}`} className="flex items-start justify-between gap-4 rounded-md px-2 py-1.5">
                  <span className="text-base text-foreground">{item.description}</span>
                  <div className="flex shrink-0 items-center gap-1">
                    {item.keys.map((key) => (
                      <kbd
                        key={`${section.title}-${item.description}-${key}`}
                        className="inline-flex min-w-7 items-center justify-center rounded-md border border-border bg-background px-2 py-1 text-xs font-medium text-content-secondary shadow-sm"
                      >
                        {key}
                      </kbd>
                    ))}
                  </div>
                </div>
              ))}
            </div>
          </section>
        ))}
      </div>
    </Modal>
  )
}

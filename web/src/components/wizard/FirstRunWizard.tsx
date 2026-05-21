import { useEffect } from 'react'
import { useWizardStore } from '@/stores/wizardStore'
import { LocateClaudeStep } from './steps/LocateClaudeStep'
import { CreateAgentStep } from './steps/CreateAgentStep'
import { TryItStep } from './steps/TryItStep'
import { cn } from '@/lib/utils'

export function FirstRunWizard() {
  const step = useWizardStore((s) => s.step)
  const complete = useWizardStore((s) => s.complete)
  const init = useWizardStore((s) => s.init)

  useEffect(() => {
    init()
  }, [init])

  if (complete) return null

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 backdrop-blur-sm">
      <div className="w-[480px] max-w-[92vw] rounded-lg border bg-card shadow-2xl">
        <header className="px-6 pt-6 pb-4 border-b">
          <h1 className="text-xl font-semibold text-foreground">Welcome to cocli local</h1>
          <div className="mt-4 flex items-center gap-2" role="group" aria-label="Progress">
            {[1, 2, 3].map((n) => (
              <span
                key={n}
                data-testid="wizard-progress-dot"
                data-active={step === n ? 'true' : 'false'}
                className={cn(
                  'h-2 w-2 rounded-full transition-colors',
                  step === n ? 'bg-primary' : 'bg-muted',
                )}
              />
            ))}
          </div>
        </header>
        <div className="px-6 py-6">
          {step === 1 && <LocateClaudeStep />}
          {step === 2 && <CreateAgentStep />}
          {step === 3 && <TryItStep />}
        </div>
      </div>
    </div>
  )
}

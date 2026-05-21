import { useNavigate } from 'react-router-dom'
import { Check } from 'lucide-react'
import { useWizardStore } from '@/stores/wizardStore'

export function TryItStep() {
  const finish = useWizardStore((s) => s.finish)
  const navigate = useNavigate()

  return (
    <div className="space-y-6">
      <div className="flex flex-col items-center text-center space-y-3">
        <div className="h-12 w-12 rounded-full bg-success/15 flex items-center justify-center">
          <Check className="h-6 w-6 text-success" />
        </div>
        <h2 className="text-lg font-medium text-foreground">You're all set!</h2>
        <p className="text-sm text-content-secondary">
          Go say hi in <span className="font-mono">#general</span>.
        </p>
      </div>
      <div className="flex flex-col gap-2 pt-2">
        <button
          type="button"
          onClick={() => {
            finish()
            navigate('/channel/general')
          }}
          className="w-full h-10 rounded bg-primary text-primary-foreground text-sm font-medium hover:bg-primary/90"
        >
          Open #general →
        </button>
        <button
          type="button"
          onClick={() => finish()}
          className="w-full h-9 rounded text-sm text-content-secondary hover:text-foreground"
        >
          Maybe later
        </button>
      </div>
    </div>
  )
}

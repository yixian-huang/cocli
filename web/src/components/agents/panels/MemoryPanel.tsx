import { MemoryTab } from '../MemoryTab'
import { useViewStore } from '@/stores/viewStore'

export function MemoryPanel({ agentId }: { agentId: string }) {
  const setAgentSubview = useViewStore((s) => s.setAgentSubview)

  return (
    <div data-testid="drawer-memory" className="flex flex-col h-full">
      <div className="flex-1 min-h-0 overflow-hidden flex flex-col">
        <MemoryTab agentId={agentId} />
      </div>
      <footer className="border-t p-2 shrink-0">
        <button
          type="button"
          onClick={() => setAgentSubview(agentId, 'settings')}
          className="w-full text-xs text-primary hover:underline py-1"
        >
          Edit in Settings →
        </button>
      </footer>
    </div>
  )
}

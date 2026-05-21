import { useEffect } from 'react'
import { useViewStore } from '@/stores/viewStore'

export function ZoneDevToolsRoute({ children }: { children: React.ReactNode }) {
  const clearActiveAgent = useViewStore((s) => s.clearActiveAgent)

  useEffect(() => {
    clearActiveAgent()
  }, [clearActiveAgent])

  return children
}


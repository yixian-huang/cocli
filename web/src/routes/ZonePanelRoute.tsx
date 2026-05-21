import { useEffect } from 'react'
import type { WorkspacePanel } from '@/stores/workspacePanelStore'
import { useWorkspacePanelStore } from '@/stores/workspacePanelStore'
import { useViewStore } from '@/stores/viewStore'

export function ZonePanelRoute({ panel }: { panel: WorkspacePanel }) {
  const setPanel = useWorkspacePanelStore((s) => s.setPanel)
  const clearActiveAgent = useViewStore((s) => s.clearActiveAgent)

  useEffect(() => {
    clearActiveAgent()
    setPanel(panel)
  }, [panel, setPanel, clearActiveAgent])

  return null
}


import { create } from 'zustand'

export type WorkspacePanel =
  | 'chat'
  | 'history'

interface WorkspacePanelState {
  panel: WorkspacePanel
  setPanel: (panel: WorkspacePanel) => void
}

export const useWorkspacePanelStore = create<WorkspacePanelState>((set) => ({
  panel: 'chat',
  setPanel: (panel) => set({ panel }),
}))

import { create } from 'zustand'

export type DialogId =
  | 'createChannel'
  | 'createAgent'
  | 'addDaemon'
  | 'createZone'
  | 'openDM'

interface DialogState {
  active: DialogId | null
  payload: Record<string, unknown> | null
  openCreateChannel: (payload?: Record<string, unknown>) => void
  openCreateAgent: (payload?: Record<string, unknown>) => void
  openAddDaemon: (payload?: Record<string, unknown>) => void
  openCreateZone: (payload?: Record<string, unknown>) => void
  openOpenDM: (payload?: Record<string, unknown>) => void
  close: () => void
}

export const useDialogStore = create<DialogState>((set) => ({
  active: null,
  payload: null,
  openCreateChannel: (payload = {}) => set({ active: 'createChannel', payload }),
  openCreateAgent: (payload = {}) => set({ active: 'createAgent', payload }),
  openAddDaemon: (payload = {}) => set({ active: 'addDaemon', payload }),
  openCreateZone: (payload = {}) => set({ active: 'createZone', payload }),
  openOpenDM: (payload = {}) => set({ active: 'openDM', payload }),
  close: () => set({ active: null, payload: null }),
}))

export function resetDialogStore() {
  useDialogStore.setState({ active: null, payload: null })
}

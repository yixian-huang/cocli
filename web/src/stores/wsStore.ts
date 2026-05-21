import { create } from 'zustand'

type WSStatus = 'connected' | 'connecting' | 'disconnected'

interface WSState {
  status: WSStatus
  setStatus: (status: WSStatus) => void
}

export const useWSStore = create<WSState>((set) => ({
  status: 'connecting',
  setStatus: (status) => set({ status }),
}))

import { create } from 'zustand'
import type { MachineVersionStatus } from '@/lib/types'

// Live overlay of machine version status, keyed by machineId. Hydrated
// not by initial fetch (each component still pulls its own list via
// daemonsApi.list) but by the "machine:updated" WS event the server
// broadcasts on every daemon ready handshake. Components render
// `overlay[machineId] ?? machine.versionStatus` so the page-load value
// is visible immediately and any subsequent push supersedes it.
//
// This is intentionally thin — no list state, no fetch logic, no
// per-zone keying. The existing per-component useState patterns own
// the list; this store only carries the volatile "is the version
// drift verdict still current" overlay, the only field that changes
// post-mount without a navigation.

export interface MachineStatusOverlay {
  daemonVersion: string
  versionStatus: MachineVersionStatus
}

interface MachineStatusStore {
  overlay: Record<string, MachineStatusOverlay>
  applyMachineUpdated: (payload: {
    machineId: string
    daemonVersion: string
    versionStatus: MachineVersionStatus
  }) => void
}

export const useMachineStatusStore = create<MachineStatusStore>((set) => ({
  overlay: {},
  applyMachineUpdated: (payload) =>
    set((state) => ({
      overlay: {
        ...state.overlay,
        [payload.machineId]: {
          daemonVersion: payload.daemonVersion,
          versionStatus: payload.versionStatus,
        },
      },
    })),
}))

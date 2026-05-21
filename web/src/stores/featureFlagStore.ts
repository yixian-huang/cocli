/**
 * Client-side feature flag store.
 *
 * Flags are probed lazily: when a skills_v2-gated component first mounts it
 * calls `probeSkillsV2()`. The probe fires one GET to /api/agents/:id/skills;
 * a 200 means the flag is on server-side, a 404/403 means off.  Components
 * can also import `setFlag` for testing overrides.
 */
import { create } from 'zustand'

interface FeatureFlagState {
  flags: Record<string, boolean>
  setFlag: (key: string, value: boolean) => void
}

export const useFeatureFlagStore = create<FeatureFlagState>((set) => ({
  flags: {},
  setFlag: (key, value) =>
    set((s) => ({ flags: { ...s.flags, [key]: value } })),
}))

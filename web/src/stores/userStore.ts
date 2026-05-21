import { create } from 'zustand'
import type { User } from '@shared/types'

interface UserState {
  user: User
  loading: boolean
  init: () => void
}

const localOwner: User = {
  id: 'local',
  name: 'owner',
  displayName: 'owner',
}

export const useUserStore = create<UserState>(() => ({
  user: localOwner,
  loading: false,
  init: () => {
    /* no-op: single-tenant local has no async user bootstrap */
  },
}))

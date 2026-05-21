import { create } from 'zustand'
import { zoneInvites as zoneInvitesApi, zoneMembers as zoneMembersApi } from '@/api/client'
import type { ZoneInvite, ZoneMember } from '@/lib/types'

interface ZoneAdminState {
  members: ZoneMember[]
  invites: ZoneInvite[]
  loadingMembers: boolean
  loadingInvites: boolean
  error: string | null
  fetchMembers: (zoneId: string) => Promise<void>
  fetchInvites: (zoneId: string) => Promise<void>
  updateMember: (zoneId: string, memberId: string, patch: Partial<ZoneMember>) => Promise<void>
  createInvite: (
    zoneId: string,
    payload: { invitedUsername?: string; expiresAt?: string; maxUses?: number },
  ) => Promise<void>
  revokeInvite: (zoneId: string, inviteId: string) => Promise<void>
}

export const useZoneAdminStore = create<ZoneAdminState>((set) => ({
  members: [],
  invites: [],
  loadingMembers: false,
  loadingInvites: false,
  error: null,

  fetchMembers: async (zoneId) => {
    set({ loadingMembers: true, error: null })
    try {
      const members = await zoneMembersApi.list(zoneId)
      set({
        members: Array.isArray(members) ? members : [],
        loadingMembers: false,
      })
    } catch (err) {
      set({
        loadingMembers: false,
        error: err instanceof Error ? err.message : 'Failed to load members',
      })
    }
  },

  fetchInvites: async (zoneId) => {
    set({ loadingInvites: true, error: null })
    try {
      const res = await zoneInvitesApi.list(zoneId)
      set({
        invites: Array.isArray(res.invites) ? res.invites : [],
        loadingInvites: false,
      })
    } catch (err) {
      set({
        loadingInvites: false,
        error: err instanceof Error ? err.message : 'Failed to load invites',
      })
    }
  },

  updateMember: async (zoneId, memberId, patch) => {
    const updated = await zoneMembersApi.updatePermissions(zoneId, memberId, patch)
    set((state) => ({
      members: state.members.map((member) => (member.userId === memberId ? { ...member, ...updated } : member)),
    }))
  },

  createInvite: async (zoneId, payload) => {
    const invite = await zoneInvitesApi.create(zoneId, payload)
    set((state) => ({
      invites: [invite, ...state.invites],
    }))
  },

  revokeInvite: async (zoneId, inviteId) => {
    await zoneInvitesApi.revoke(zoneId, inviteId)
    set((state) => ({
      invites: state.invites.map((invite) =>
        invite.id === inviteId ? { ...invite, status: 'revoked' } : invite,
      ),
    }))
  },
}))

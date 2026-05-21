import { useEffect, useMemo, useState } from 'react'
import { useZoneStore } from '@/stores/zoneStore'
import { useZoneAdminStore } from '@/stores/zoneAdminStore'
import { useUserStore } from '@/stores/userStore'
import { toast, toastError } from '@/stores/toastStore'
import { Button, Badge } from '@/components/ui'
import type { ZoneMember } from '@/lib/types'
import { Plus, RefreshCw } from 'lucide-react'
import { ZoneThemeSelect } from './ZoneThemeSelect'

function boolLabel(value?: boolean) {
  return value ? 'on' : 'off'
}

function memberDisplayName(member: ZoneMember) {
  return member.userDisplayName || member.userName || member.userId
}

export function ZoneMembersPanel() {
  const activeZoneId = useZoneStore((s) => s.activeZoneId)
  const currentUser = useUserStore((s) => s.user)
  const isAdmin = currentUser?.role === 'admin'

  const members = useZoneAdminStore((s) => s.members)
  const invites = useZoneAdminStore((s) => s.invites)
  const loadingMembers = useZoneAdminStore((s) => s.loadingMembers)
  const loadingInvites = useZoneAdminStore((s) => s.loadingInvites)
  const error = useZoneAdminStore((s) => s.error)
  const fetchMembers = useZoneAdminStore((s) => s.fetchMembers)
  const fetchInvites = useZoneAdminStore((s) => s.fetchInvites)
  const updateMember = useZoneAdminStore((s) => s.updateMember)
  const createInvite = useZoneAdminStore((s) => s.createInvite)
  const revokeInvite = useZoneAdminStore((s) => s.revokeInvite)

  const [invitedUsername, setInvitedUsername] = useState('')
  const [maxUses, setMaxUses] = useState('1')
  const [expiresAt, setExpiresAt] = useState('')
  const [creatingInvite, setCreatingInvite] = useState(false)

  useEffect(() => {
    if (!activeZoneId) return
    fetchMembers(activeZoneId)
    fetchInvites(activeZoneId)
  }, [activeZoneId, fetchMembers, fetchInvites])

  const sortedMembers = useMemo(
    () =>
      [...members].sort((a, b) => {
        if (a.role === 'admin' && b.role !== 'admin') return -1
        if (a.role !== 'admin' && b.role === 'admin') return 1
        return memberDisplayName(a).localeCompare(memberDisplayName(b))
      }),
    [members],
  )

  if (!activeZoneId) return null

  return (
    <div className="flex-1 min-h-0 flex flex-col">
      <div className="h-12 border-b px-4 flex items-center justify-between">
        <div className="text-sm font-semibold">Zone Members & Invites</div>
        <Button
          variant="ghost"
          size="sm"
          className="gap-1"
          onClick={() => {
            fetchMembers(activeZoneId)
            fetchInvites(activeZoneId)
          }}
        >
          <RefreshCw className="h-3.5 w-3.5" />
          Refresh
        </Button>
      </div>

      {error && <div className="px-4 py-2 text-xs text-error">{error}</div>}

      <div className="flex-1 min-h-0 overflow-y-auto">
        <div className="px-4 py-3 border-b">
          <div className="text-xs font-semibold text-muted-foreground mb-2">Permission matrix</div>
          {loadingMembers ? (
            <div className="text-xs text-muted-foreground">Loading members...</div>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-xs">
                <thead>
                  <tr className="text-muted-foreground">
                    <th className="text-left py-1.5">Member</th>
                    <th className="text-left py-1.5">Role</th>
                    <th className="text-left py-1.5">Create Channel</th>
                    <th className="text-left py-1.5">Create Agent</th>
                    <th className="text-left py-1.5">Invite</th>
                    <th className="text-left py-1.5">Hide From Agents</th>
                  </tr>
                </thead>
                <tbody>
                  {sortedMembers.map((member) => {
                    const disabled = !isAdmin || member.role === 'admin'
                    const userId = member.userId || member.id
                    return (
                      <tr key={member.id} className="border-t">
                        <td className="py-2 pr-2">
                          <div className="font-medium">{memberDisplayName(member)}</div>
                          <div className="text-[11px] text-muted-foreground">{member.userId}</div>
                        </td>
                        <td className="py-2 pr-2">
                          <Badge size="sm" variant={member.role === 'admin' ? 'info' : 'default'}>
                            {member.role}
                          </Badge>
                        </td>
                        <td className="py-2 pr-2">
                          <label className="inline-flex items-center gap-1">
                            <input
                              type="checkbox"
                              checked={!!member.canCreateChannel || member.role === 'admin'}
                              disabled={disabled}
                              onChange={async (e) => {
                                try {
                                  await updateMember(activeZoneId, userId, { canCreateChannel: e.target.checked })
                                } catch (err) {
                                  toastError(err instanceof Error ? err.message : 'Failed to update member')
                                }
                              }}
                            />
                            <span>{boolLabel(member.canCreateChannel || member.role === 'admin')}</span>
                          </label>
                        </td>
                        <td className="py-2 pr-2">
                          <label className="inline-flex items-center gap-1">
                            <input
                              type="checkbox"
                              checked={!!member.canCreateAgent || member.role === 'admin'}
                              disabled={disabled}
                              onChange={async (e) => {
                                try {
                                  await updateMember(activeZoneId, userId, { canCreateAgent: e.target.checked })
                                } catch (err) {
                                  toastError(err instanceof Error ? err.message : 'Failed to update member')
                                }
                              }}
                            />
                            <span>{boolLabel(member.canCreateAgent || member.role === 'admin')}</span>
                          </label>
                        </td>
                        <td className="py-2 pr-2">
                          <label className="inline-flex items-center gap-1">
                            <input
                              type="checkbox"
                              checked={!!member.canInviteOthers || member.role === 'admin'}
                              disabled={disabled}
                              onChange={async (e) => {
                                try {
                                  await updateMember(activeZoneId, userId, { canInviteOthers: e.target.checked })
                                } catch (err) {
                                  toastError(err instanceof Error ? err.message : 'Failed to update member')
                                }
                              }}
                            />
                            <span>{boolLabel(member.canInviteOthers || member.role === 'admin')}</span>
                          </label>
                        </td>
                        <td className="py-2 pr-2">
                          <label className="inline-flex items-center gap-1">
                            <input
                              type="checkbox"
                              checked={!!member.hideFromAgents}
                              disabled={!isAdmin}
                              onChange={async (e) => {
                                try {
                                  await updateMember(activeZoneId, userId, { hideFromAgents: e.target.checked })
                                } catch (err) {
                                  toastError(err instanceof Error ? err.message : 'Failed to update member')
                                }
                              }}
                            />
                            <span>{boolLabel(member.hideFromAgents)}</span>
                          </label>
                        </td>
                      </tr>
                    )
                  })}
                </tbody>
              </table>
            </div>
          )}
        </div>

        <div className="px-4 py-3">
          <div className="text-xs font-semibold text-muted-foreground mb-2">Invite management</div>
          {isAdmin ? (
            <form
              className="grid grid-cols-1 md:grid-cols-4 gap-2 mb-3"
              onSubmit={async (e) => {
                e.preventDefault()
                if (creatingInvite) return
                setCreatingInvite(true)
                try {
                  await createInvite(activeZoneId, {
                    invitedUsername: invitedUsername || undefined,
                    maxUses: maxUses ? Number(maxUses) : undefined,
                    expiresAt: expiresAt || undefined,
                  })
                  setInvitedUsername('')
                  setMaxUses('1')
                  setExpiresAt('')
                  toast('Invite created', 'success')
                } catch (err) {
                  toastError(err instanceof Error ? err.message : 'Failed to create invite')
                } finally {
                  setCreatingInvite(false)
                }
              }}
            >
              <input
                className="border rounded px-2 py-1 bg-background text-xs"
                placeholder="invited username (optional)"
                value={invitedUsername}
                onChange={(e) => setInvitedUsername(e.target.value)}
              />
              <input
                className="border rounded px-2 py-1 bg-background text-xs"
                type="number"
                min={1}
                placeholder="max uses"
                value={maxUses}
                onChange={(e) => setMaxUses(e.target.value)}
              />
              <input
                className="border rounded px-2 py-1 bg-background text-xs"
                type="datetime-local"
                value={expiresAt}
                onChange={(e) => setExpiresAt(e.target.value)}
              />
              <Button type="submit" size="sm" className="gap-1" disabled={creatingInvite}>
                <Plus className="h-3.5 w-3.5" />
                Create Invite
              </Button>
            </form>
          ) : (
            <div className="text-xs text-muted-foreground mb-3">Invite operations require admin permissions.</div>
          )}

          {loadingInvites ? (
            <div className="text-xs text-muted-foreground">Loading invites...</div>
          ) : invites.length === 0 ? (
            <div className="text-xs text-muted-foreground">No invites yet</div>
          ) : (
            <div className="space-y-2">
              {invites.map((invite) => (
                <div key={invite.id} className="rounded border px-2 py-2 text-xs flex items-center gap-2">
                  <code className="font-mono">{invite.code}</code>
                  <Badge
                    size="sm"
                    variant={invite.status === 'active' ? 'success' : invite.status === 'used' ? 'info' : 'default'}
                  >
                    {invite.status}
                  </Badge>
                  <span className="text-muted-foreground">
                    {invite.invitedUsername ? `for ${invite.invitedUsername}` : 'open invite'}
                  </span>
                  <span className="ml-auto text-muted-foreground">
                    created {new Date(invite.createdAt).toLocaleString()}
                  </span>
                  {isAdmin && invite.status === 'active' && (
                    <Button
                      size="sm"
                      variant="secondary"
                      onClick={async () => {
                        try {
                          await revokeInvite(activeZoneId, invite.id)
                          toast('Invite revoked', 'success')
                        } catch (err) {
                          toastError(err instanceof Error ? err.message : 'Failed to revoke invite')
                        }
                      }}
                    >
                      Revoke
                    </Button>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>

        {isAdmin && (
          <div className="px-4 py-3 border-t border-border-default">
            <div className="text-xs font-semibold text-muted-foreground mb-2">Zone settings</div>
            <ZoneThemeSelect />
          </div>
        )}
      </div>
    </div>
  )
}

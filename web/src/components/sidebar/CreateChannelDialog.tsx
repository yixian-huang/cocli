import { useEffect, useMemo, useState } from 'react'
import { Modal, Button, Input, Textarea } from '@/components/ui'
import { useDialogStore } from '@/stores/dialogStore'
import { channels as channelsApi } from '@/api/client'
import { useChannelStore } from '@/stores/channelStore'
import { useAgentStore } from '@/stores/agentStore'
import { useUserStore } from '@/stores/userStore'
import { toastError } from '@/stores/toastStore'
import { useZoneStore } from '@/stores/zoneStore'

export function CreateChannelDialog() {
  const active = useDialogStore((s) => s.active)
  const payload = useDialogStore((s) => s.payload)
  const close = useDialogStore((s) => s.close)
  const open = active === 'createChannel'

  const agents = useAgentStore((s) => s.agents)
  const agentsLoading = useAgentStore((s) => s.loading)
  const users = useUserStore((s) => s.allUsers)
  const activeZoneId = useZoneStore((s) => s.activeZoneId)
  const payloadZoneId = (payload as { zoneId?: string } | null)?.zoneId || null

  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [memberQuery, setMemberQuery] = useState('')
  const [selectedAgentIds, setSelectedAgentIds] = useState<Set<string>>(() => new Set())
  const [selectedUserIds, setSelectedUserIds] = useState<Set<string>>(() => new Set())
  const [usersLoading, setUsersLoading] = useState(false)
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!open) {
      setName('')
      setDescription('')
      setMemberQuery('')
      setSelectedAgentIds(new Set())
      setSelectedUserIds(new Set())
      setError(null)
      setSubmitting(false)
    }
  }, [open])

  useEffect(() => {
    if (!open) return
    // Guard against cross-zone confusion: member lists are zone-scoped.
    if (!payloadZoneId || !activeZoneId || payloadZoneId !== activeZoneId) return
    if (agents.length === 0) {
      void useAgentStore.getState().fetchAgents()
    }
    if (users.length === 0) {
      setUsersLoading(true)
      void useUserStore.getState().fetchUsers().finally(() => setUsersLoading(false))
    }
  }, [open, agents.length, users.length, payloadZoneId, activeZoneId])

  const text = memberQuery.trim().toLowerCase()
  const visibleAgents = useMemo(
    () => (text ? agents.filter((a) => a.name.toLowerCase().includes(text)) : agents),
    [agents, text],
  )
  const visibleUsers = useMemo(
    () =>
      text
        ? users.filter(
            (u) =>
              u.name.toLowerCase().includes(text) ||
              (u.displayName?.toLowerCase().includes(text) ?? false),
          )
        : users,
    [users, text],
  )

  const selectedCount = selectedAgentIds.size + selectedUserIds.size

  const submit = async () => {
    const zoneId = payloadZoneId
    if (!zoneId || !name.trim()) {
      setError('Name is required')
      return
    }
    setSubmitting(true)
    setError(null)
    try {
      const channel = await channelsApi.create(zoneId, name.trim(), description)
      const members: Array<{ memberId: string; memberType: string }> = [
        ...Array.from(selectedAgentIds).map((id) => ({ memberId: id, memberType: 'agent' })),
        ...Array.from(selectedUserIds).map((id) => ({ memberId: id, memberType: 'user' })),
      ]
      if (members.length > 0) {
        const results = await Promise.allSettled(
          members.map((m) => channelsApi.addMember(channel.id, m.memberId, m.memberType)),
        )
        const failed = results.filter((r) => r.status === 'rejected')
        if (failed.length > 0) {
          toastError(`Created channel, but failed to add ${failed.length} member(s).`)
        }
      }
      await useChannelStore.getState().fetchChannels()
      close()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create channel')
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <Modal
      open={open}
      onClose={close}
      title="New channel"
      footer={
        <>
          <Button variant="ghost" onClick={close} disabled={submitting}>Cancel</Button>
          <Button onClick={submit} disabled={submitting}>
            {submitting ? 'Creating…' : 'Create'}
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        <label className="block text-sm">
          <span className="text-muted-foreground">Name</span>
          <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="alpha-eng" autoFocus />
        </label>
        <label className="block text-sm">
          <span className="text-muted-foreground">Description (optional)</span>
          <Textarea value={description} onChange={(e) => setDescription(e.target.value)} rows={2} />
        </label>
        <div className="space-y-2 rounded border border-border/60 p-3">
          <div className="flex items-center justify-between gap-3">
            <div className="text-sm font-medium">Members (optional)</div>
            {selectedCount > 0 && <div className="text-xs text-muted-foreground">{selectedCount} selected</div>}
          </div>
          {payloadZoneId && activeZoneId && payloadZoneId !== activeZoneId ? (
            <div className="text-xs text-warning-emphasis">
              Switch to the current zone to pick members.
            </div>
          ) : null}
          <Input
            value={memberQuery}
            onChange={(e) => setMemberQuery(e.target.value)}
            placeholder="Search agents/users…"
            aria-label="Search members"
          />
          <div className="space-y-3">
            <div className="space-y-1">
              <div className="text-xs font-medium text-muted-foreground">Agents</div>
              <div className="space-y-1">
                {visibleAgents.length === 0 ? (
                  <div className="text-xs text-muted-foreground px-1 py-1">
                    {agentsLoading ? 'Loading…' : 'No agents'}
                  </div>
                ) : (
                  visibleAgents.map((a) => {
                    const checked = selectedAgentIds.has(a.id)
                    return (
                      <label
                        key={a.id}
                        className="flex items-center gap-2 rounded px-2 py-1 text-sm hover:bg-accent/40"
                      >
                        <input
                          type="checkbox"
                          checked={checked}
                          onChange={(e) => {
                            setSelectedAgentIds((prev) => {
                              const next = new Set(prev)
                              if (e.target.checked) next.add(a.id)
                              else next.delete(a.id)
                              return next
                            })
                          }}
                        />
                        <span className="truncate">@{a.name}</span>
                      </label>
                    )
                  })
                )}
              </div>
            </div>
            <div className="space-y-1">
              <div className="text-xs font-medium text-muted-foreground">Users</div>
              <div className="space-y-1">
                {visibleUsers.length === 0 ? (
                  <div className="text-xs text-muted-foreground px-1 py-1">
                    {usersLoading ? 'Loading…' : 'No users'}
                  </div>
                ) : (
                  visibleUsers.map((u) => {
                    const checked = selectedUserIds.has(u.id)
                    const label = u.displayName || u.name
                    return (
                      <label
                        key={u.id}
                        className="flex items-center gap-2 rounded px-2 py-1 text-sm hover:bg-accent/40"
                      >
                        <input
                          type="checkbox"
                          checked={checked}
                          onChange={(e) => {
                            setSelectedUserIds((prev) => {
                              const next = new Set(prev)
                              if (e.target.checked) next.add(u.id)
                              else next.delete(u.id)
                              return next
                            })
                          }}
                        />
                        <span className="truncate">{label}</span>
                      </label>
                    )
                  })
                )}
              </div>
            </div>
          </div>
        </div>
        {error && <div className="text-sm text-destructive">{error}</div>}
      </div>
    </Modal>
  )
}

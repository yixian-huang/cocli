import { useState, useEffect } from 'react'
import { invites as invitesApi } from '@/api/client'
import { useUserStore } from '@/stores/userStore'
import { toast, toastError } from '@/stores/toastStore'
import type { Invite } from '@/lib/types'
import { Copy, Trash2, Plus, X } from 'lucide-react'
import { Button, Input, Select, SectionHeader } from '@/components/ui'

export function InviteLinks() {
  const isAdmin = useUserStore((s) => s.user?.role === 'admin')
  const [inviteList, setInviteList] = useState<Invite[]>([])
  const [showForm, setShowForm] = useState(false)
  const [maxUses, setMaxUses] = useState('')
  const [expiresIn, setExpiresIn] = useState('168h')

  useEffect(() => {
    if (isAdmin) {
      invitesApi.list().then((data) => setInviteList(data.invites)).catch((err) => console.warn('[api] invites fetch failed:', err))
    }
  }, [isAdmin])

  if (!isAdmin) return null

  const handleCreate = async () => {
    try {
      const invite = await invitesApi.create(
        maxUses ? parseInt(maxUses) : undefined,
        expiresIn || undefined
      )
      setInviteList((prev) => [invite, ...prev])
      setShowForm(false)
      setMaxUses('')
      toast('Invite created', 'success')
    } catch (err) {
      toastError(err instanceof Error ? err.message : 'Failed to create invite')
    }
  }

  const handleRevoke = async (id: string) => {
    try {
      await invitesApi.revoke(id)
      setInviteList((prev) => prev.filter((inv) => inv.id !== id))
      toast('Invite revoked', 'success')
    } catch (err) {
      toastError(err instanceof Error ? err.message : 'Failed to revoke invite')
    }
  }

  const copyUrl = (code: string) => {
    const url = `${window.location.origin}/#invite=${code}`
    navigator.clipboard.writeText(url)
    toast('Copied invite link', 'success')
  }

  return (
    <div className="px-2 py-2 border-t border-border mt-2">
      <SectionHeader title="Invite Links" />

      {!showForm ? (
        <button
          onClick={() => setShowForm(true)}
          className="flex items-center gap-1 px-2 py-1 text-xs text-primary hover:text-primary/80 transition-colors"
        >
          <Plus className="h-3 w-3" /> Create invite link
        </button>
      ) : (
        <div className="px-2 py-2 space-y-2">
          <Input
            type="number"
            value={maxUses}
            onChange={(e) => setMaxUses(e.target.value)}
            placeholder="Max uses (optional)"
            className="text-xs"
          />
          <Select
            value={expiresIn}
            onChange={(e) => setExpiresIn(e.target.value)}
            options={[
              { value: '24h', label: '1 day' },
              { value: '168h', label: '7 days' },
              { value: '720h', label: '30 days' },
              { value: '', label: 'Never' },
            ]}
            className="text-xs"
          />
          <div className="flex gap-1">
            <Button variant="primary" size="sm" onClick={handleCreate} className="flex-1">Create</Button>
            <Button variant="secondary" size="sm" onClick={() => setShowForm(false)}><X className="h-3 w-3" /></Button>
          </div>
        </div>
      )}

      {inviteList.map((inv) => (
        <div key={inv.id} className="flex items-center gap-1 px-2 py-1 text-xs text-foreground">
          <span className="truncate flex-1 font-mono">{inv.code.slice(0, 8)}...</span>
          <span className="text-[10px] text-muted-foreground shrink-0">
            {inv.useCount}{inv.maxUses ? `/${inv.maxUses}` : ''} uses
          </span>
          <button onClick={() => copyUrl(inv.code)} className="p-0.5 hover:bg-accent rounded" title="Copy link">
            <Copy className="h-3 w-3 text-muted-foreground" />
          </button>
          <button onClick={() => handleRevoke(inv.id)} className="p-0.5 hover:bg-accent rounded" title="Revoke">
            <Trash2 className="h-3 w-3 text-muted-foreground" />
          </button>
        </div>
      ))}
    </div>
  )
}

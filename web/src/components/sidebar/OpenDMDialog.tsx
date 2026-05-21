import { useEffect, useState } from 'react'
import { Modal, Button, Input } from '@/components/ui'
import { useDialogStore } from '@/stores/dialogStore'
import { dm as dmApi } from '@/api/client'
import { useChannelStore } from '@/stores/channelStore'

export function OpenDMDialog() {
  const open = useDialogStore((s) => s.active === 'openDM')
  const close = useDialogStore((s) => s.close)
  const [recipient, setRecipient] = useState('')
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!open) {
      setRecipient('')
      setSubmitting(false)
      setError(null)
    }
  }, [open])

  const submit = async () => {
    const trimmed = recipient.trim().replace(/^@/, '')
    if (!trimmed) {
      setError('Recipient required')
      return
    }
    setSubmitting(true)
    setError(null)
    try {
      await dmApi.createOrGet(trimmed)
      await useChannelStore.getState().fetchDMs()
      close()
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to open DM')
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <Modal
      open={open}
      onClose={close}
      title="Open DM"
      footer={
        <>
          <Button variant="ghost" onClick={close} disabled={submitting}>Cancel</Button>
          <Button onClick={submit} disabled={submitting}>
            {submitting ? 'Opening…' : 'Open'}
          </Button>
        </>
      }
    >
      <label className="block text-sm">
        <span className="text-muted-foreground">Recipient (user name)</span>
        <Input
          value={recipient}
          onChange={(e) => setRecipient(e.target.value)}
          placeholder="@username"
          autoFocus
        />
      </label>
      {error && <div className="mt-2 text-sm text-destructive">{error}</div>}
    </Modal>
  )
}

import { useEffect, useState } from 'react'
import { Modal, Button, Input } from '@/components/ui'
import { useDialogStore } from '@/stores/dialogStore'
import { useZoneStore } from '@/stores/zoneStore'

export function CreateZoneDialog() {
  const open = useDialogStore((s) => s.active === 'createZone')
  const close = useDialogStore((s) => s.close)
  const createZone = useZoneStore((s) => s.createZone)
  const setActiveZone = useZoneStore((s) => s.setActiveZone)

  const [name, setName] = useState('')
  const [slug, setSlug] = useState('')
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!open) {
      setName('')
      setSlug('')
      setSubmitting(false)
      setError(null)
    }
  }, [open])

  const validateSlug = (value: string) => {
    const s = value.trim()
    if (!s) return 'Slug is required'
    if (s.length < 5 || s.length > 8) return 'Slug must be 5-8 characters'
    if (!/^[A-Za-z0-9-]+$/.test(s)) return 'Slug can only contain letters, numbers, and hyphens'
    return null
  }

  const submit = async () => {
    if (!name.trim()) {
      setError('Name is required')
      return
    }
    const slugErr = validateSlug(slug)
    if (slugErr) {
      setError(slugErr)
      return
    }
    setSubmitting(true)
    setError(null)
    try {
      const zone = await createZone(name.trim(), slug.trim())
      setActiveZone(zone.id)
      close()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create zone')
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <Modal
      open={open}
      onClose={close}
      title="New zone"
      footer={
        <>
          <Button variant="ghost" onClick={close} disabled={submitting}>Cancel</Button>
          <Button onClick={submit} disabled={submitting}>
            {submitting ? 'Creating…' : 'Create'}
          </Button>
        </>
      }
    >
      <label className="block text-sm">
        <span className="text-muted-foreground">Name</span>
        <Input
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="my-zone"
          autoFocus
        />
      </label>
      <label className="block text-sm mt-3">
        <span className="text-muted-foreground">Slug</span>
        <Input
          value={slug}
          onChange={(e) => setSlug(e.target.value)}
          placeholder="abc-12"
          spellCheck={false}
        />
        <div className="mt-1 text-[11px] text-muted-foreground">
          5-8 chars, letters/numbers/hyphen only, must be unique
        </div>
      </label>
      {error && <div className="mt-2 text-sm text-destructive">{error}</div>}
    </Modal>
  )
}

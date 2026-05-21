import { useState } from 'react'
import { Modal, Button, Input } from '@/components/ui'
import { zoneSkillLibrary, ApiError } from '@/api/client'

interface Props {
  open: boolean
  zoneId: string
  onClose: () => void
  onImported: (libraryId: string) => void
}

interface ConflictState {
  existingId: string
  existingSource: string
}

export function SkillsLibraryImportModal({ open, zoneId, onClose, onImported }: Props) {
  const [url, setUrl] = useState('')
  const [subPath, setSubPath] = useState('')
  const [name, setName] = useState('')
  const [importing, setImporting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [conflict, setConflict] = useState<ConflictState | null>(null)
  const [controller, setController] = useState<AbortController | null>(null)

  const reset = () => {
    setUrl(''); setSubPath(''); setName('')
    setImporting(false); setError(null); setConflict(null); setController(null)
  }

  const close = () => { reset(); onClose() }

  const doImport = async () => {
    if (!url.trim()) return
    setError(null); setConflict(null)
    const ac = new AbortController()
    setController(ac)
    setImporting(true)
    try {
      const res = await zoneSkillLibrary.import(
        zoneId,
        { url: url.trim(), subPath: subPath.trim() || undefined, name: name.trim() || undefined },
        { signal: ac.signal },
      )
      onImported(res.library_id)
      reset()
    } catch (e) {
      if ((e as DOMException)?.name === 'AbortError') {
        // user cancelled — just reset to idle, keep modal open
        setImporting(false); setController(null)
        return
      }
      const apiErr = e as ApiError
      if (apiErr?.status === 409 && apiErr.body) {
        try {
          const body = JSON.parse(apiErr.body) as { existing_source?: string; existing_id?: string }
          if (body.existing_id && body.existing_source) {
            setConflict({ existingId: body.existing_id, existingSource: body.existing_source })
            setImporting(false); setController(null)
            return
          }
        } catch { /* fall through */ }
      }
      setError(apiErr?.message || 'Import failed')
      setImporting(false); setController(null)
    }
  }

  const doReinstall = async () => {
    if (!conflict) return
    setImporting(true); setError(null)
    try {
      await zoneSkillLibrary.reinstall(zoneId, conflict.existingId)
      onImported(conflict.existingId)
      reset()
    } catch (e) {
      setError((e as Error).message || 'Reinstall failed')
      setImporting(false)
    }
  }

  const cancel = () => {
    controller?.abort()
  }

  const footer = importing ? (
    <Button variant="secondary" size="sm" onClick={cancel}>Cancel</Button>
  ) : conflict ? (
    <>
      <Button variant="ghost" size="sm" onClick={close}>Close</Button>
      <Button variant="primary" size="sm" onClick={doReinstall}>Reinstall</Button>
    </>
  ) : (
    <>
      <Button variant="ghost" size="sm" onClick={close}>Cancel</Button>
      <Button variant="primary" size="sm" onClick={doImport} disabled={!url.trim()}>Import</Button>
    </>
  )

  return (
    <Modal open={open} onClose={close} title="Import skill from URL" size="md" footer={footer}>
      <div className="space-y-3">
        <Input
          label="URL"
          placeholder="https://github.com/org/repo  or  https://example.com/skill.zip"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          disabled={importing}
          aria-label="URL"
        />
        <Input
          label="Subpath (optional)"
          placeholder="skills/my-skill"
          value={subPath}
          onChange={(e) => setSubPath(e.target.value)}
          disabled={importing}
          aria-label="Subpath"
        />
        <Input
          label="Name (optional — auto-derived if blank)"
          placeholder="my-skill"
          value={name}
          onChange={(e) => setName(e.target.value)}
          disabled={importing}
          aria-label="Name"
        />
        {importing && (
          <p className="text-sm text-muted-foreground">Importing… (fetching, validating, storing)</p>
        )}
        {error && (
          <p className="text-sm text-destructive">{error}</p>
        )}
        {conflict && (
          <div className="rounded border border-amber-500/40 bg-amber-500/10 p-3 text-sm">
            <p>A skill with this name already exists.</p>
            <p className="text-xs text-muted-foreground mt-1">
              Existing source: <span className="font-mono">{conflict.existingSource}</span>
            </p>
            <p className="mt-2">Click <strong>Reinstall</strong> to fetch the latest version and overwrite, or <strong>Close</strong> and pick a different name.</p>
          </div>
        )}
      </div>
    </Modal>
  )
}

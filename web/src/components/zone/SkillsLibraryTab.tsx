import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Plus, RefreshCw, Search, Trash2, RotateCw, Eye, Download, Loader2, AlertCircle } from 'lucide-react'
import { Button, Input, Badge, EmptyState } from '@/components/ui'
import { zoneSkillLibrary } from '@/api/client'
import type { SkillLibraryEntry } from '@/lib/types'
import { cn } from '@/lib/utils'
import { SkillsLibraryImportModal } from './SkillsLibraryImportModal'
import { SkillsLibraryDetailModal } from './SkillsLibraryDetailModal'

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`
}

function shortRef(ref?: string): string {
  if (!ref) return '—'
  // Treat 40-char hex as a commit SHA → truncate to 7 chars; anything
  // shorter is presumed a tag/branch name and shown verbatim.
  if (/^[0-9a-f]{40}$/i.test(ref)) return ref.slice(0, 7)
  return ref
}

export function SkillsLibraryTab({ zoneId }: { zoneId: string }) {
  const [entries, setEntries] = useState<SkillLibraryEntry[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [query, setQuery] = useState('')
  const [importOpen, setImportOpen] = useState(false)
  const [detailFor, setDetailFor] = useState<SkillLibraryEntry | null>(null)
  const refreshGeneration = useRef(0)

  const refresh = useCallback(async () => {
    const generation = ++refreshGeneration.current
    setLoading(true)
    setError(null)
    try {
      const response = await zoneSkillLibrary.list(zoneId)
      if (generation === refreshGeneration.current) {
        setEntries(response.entries || [])
      }
    } catch (error) {
      if (generation === refreshGeneration.current) {
        setError((error as Error)?.message || 'Failed to load library')
      }
    } finally {
      if (generation === refreshGeneration.current) {
        setLoading(false)
      }
    }
  }, [zoneId])

  useEffect(() => {
    void refresh()
    return () => {
      refreshGeneration.current += 1
    }
  }, [refresh])

  const filtered = useMemo(() => {
    if (!query.trim()) return entries
    const q = query.trim().toLowerCase()
    return entries.filter(
      (e) =>
        e.name.toLowerCase().includes(q) ||
        (e.description ?? '').toLowerCase().includes(q) ||
        e.sourceUrl.toLowerCase().includes(q),
    )
  }, [entries, query])

  const onReinstall = async (id: string) => {
    try {
      await zoneSkillLibrary.reinstall(zoneId, id)
      await refresh()
    } catch (e) {
      setError((e as Error).message)
    }
  }

  const onDelete = async (id: string) => {
    if (!window.confirm('Delete this library entry? Installed agents will lose the skill on their next sync.')) return
    try {
      await zoneSkillLibrary.remove(zoneId, id)
      await refresh()
    } catch (e) {
      setError((e as Error).message)
    }
  }

  if (loading) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
      </div>
    )
  }

  return (
    <div className="flex-1 flex flex-col min-h-0 overflow-hidden">
      {/* Toolbar */}
      <div className="flex items-center gap-2 p-3 border-b border-border">
        <Input
          placeholder="Search by name, description, or URL"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          iconLeft={<Search className="h-3.5 w-3.5" />}
          className="max-w-xs"
        />
        <div className="flex-1" />
        <Button variant="secondary" size="sm" onClick={refresh}>
          <RefreshCw className="h-3.5 w-3.5" /> Check updates
        </Button>
        <Button variant="primary" size="sm" onClick={() => setImportOpen(true)}>
          <Plus className="h-3.5 w-3.5" /> Import URL
        </Button>
      </div>

      {error && (
        <div className="px-3 py-2 text-sm text-destructive border-b border-destructive/40 bg-destructive/10 flex items-center gap-2">
          <AlertCircle className="h-4 w-4" /> {error}
        </div>
      )}

      {/* Table */}
      <div className="flex-1 overflow-y-auto">
        {filtered.length === 0 ? (
          <EmptyState
            icon={<Download className="h-8 w-8 opacity-40" />}
            title={query ? 'No matches' : 'No skills imported yet'}
            description={query ? 'Try a different search term.' : 'Click [+ Import URL] to add your first skill.'}
          />
        ) : (
          <table className="w-full text-sm">
            <thead className="sticky top-0 bg-background border-b border-border text-xs text-muted-foreground uppercase tracking-wider">
              <tr>
                <th className="text-left px-3 py-2 font-medium">Skill</th>
                <th className="text-left px-3 py-2 font-medium">Source</th>
                <th className="text-left px-3 py-2 font-medium">Version</th>
                <th className="text-left px-3 py-2 font-medium">Size · Files</th>
                <th className="text-left px-3 py-2 font-medium">In use</th>
                <th className="text-right px-3 py-2 font-medium">Actions</th>
              </tr>
            </thead>
            <tbody>
              {filtered.map((e) => {
                const dim = (e.inUseCount ?? 0) === 0
                return (
                  <tr
                    key={e.id}
                    className={cn(
                      'border-b border-border hover:bg-accent/40 cursor-pointer',
                      dim && 'opacity-65',
                    )}
                    onClick={() => setDetailFor(e)}
                  >
                    <td className="px-3 py-2">
                      <div className="font-medium">{e.displayName || e.name}</div>
                      <div className="text-xs text-muted-foreground">
                        imported {new Date(e.importedAt).toLocaleDateString()}
                      </div>
                    </td>
                    <td className="px-3 py-2">
                      <div className="font-mono text-xs truncate max-w-[280px]" title={e.sourceUrl}>
                        {e.sourceUrl}
                      </div>
                      {e.sourceSubpath && (
                        <div className="text-[10px] text-muted-foreground font-mono">/{e.sourceSubpath}</div>
                      )}
                    </td>
                    <td className="px-3 py-2">
                      <Badge variant="info" size="sm">{shortRef(e.sourceRef)}</Badge>
                    </td>
                    <td className="px-3 py-2 text-xs text-muted-foreground">
                      {formatSize(e.totalBytes)} · {e.fileCount} files
                    </td>
                    <td className="px-3 py-2">
                      <button
                        className="text-primary hover:underline text-xs"
                        onClick={(ev) => { ev.stopPropagation(); setDetailFor(e) }}
                      >
                        <strong>{e.inUseCount ?? 0}</strong> agents
                      </button>
                    </td>
                    <td className="px-3 py-2 text-right">
                      <div className="inline-flex items-center gap-1" onClick={(ev) => ev.stopPropagation()}>
                        <button
                          className="p-1.5 rounded hover:bg-accent text-muted-foreground hover:text-foreground"
                          title="View details"
                          onClick={() => setDetailFor(e)}
                          aria-label={`View ${e.name}`}
                        >
                          <Eye className="h-3.5 w-3.5" />
                        </button>
                        <button
                          className="p-1.5 rounded hover:bg-accent text-muted-foreground hover:text-foreground"
                          title="Reinstall (fetch latest)"
                          onClick={() => onReinstall(e.id)}
                          aria-label={`Reinstall ${e.name}`}
                        >
                          <RotateCw className="h-3.5 w-3.5" />
                        </button>
                        <button
                          className="p-1.5 rounded hover:bg-destructive/20 text-muted-foreground hover:text-destructive"
                          title="Delete"
                          onClick={() => onDelete(e.id)}
                          aria-label={`Delete ${e.name}`}
                        >
                          <Trash2 className="h-3.5 w-3.5" />
                        </button>
                      </div>
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        )}
      </div>

      <SkillsLibraryImportModal
        open={importOpen}
        zoneId={zoneId}
        onClose={() => setImportOpen(false)}
        onImported={() => { setImportOpen(false); refresh() }}
      />
      <SkillsLibraryDetailModal
        open={detailFor !== null}
        zoneId={zoneId}
        entry={detailFor}
        onClose={() => setDetailFor(null)}
      />
    </div>
  )
}

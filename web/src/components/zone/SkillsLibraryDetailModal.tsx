/**
 * SkillsLibraryDetailModal — shows a library entry's SKILL.md preview
 * plus the list of agents that have it installed.
 *
 * Phase 2 stub: `useInUseAgents` always resolves to []. The
 * agent_skill_installs table (migration 000063) exists but is empty
 * until Phase 3 (daemon install/uninstall) writes to it. When Phase 3
 * lands, replace `useInUseAgents` with a real call to
 * `agentSkills.installsByLibrary(zoneId, libraryId)` (Task TBD in
 * phase-3 plan) — the rest of this component already handles a
 * populated list.
 */
import { useEffect, useState } from 'react'
import { Modal } from '@/components/ui'
import { zoneSkillLibrary } from '@/api/client'
import type { SkillLibraryEntry, SkillLibraryFileMeta } from '@/lib/types'
import { Loader2 } from 'lucide-react'

interface Props {
  open: boolean
  zoneId: string
  entry: SkillLibraryEntry | null
  onClose: () => void
}

interface InUseAgent {
  id: string
  name: string
  installedAt: string
}

// Phase 2 stub. Phase 3 replaces with a real fetch against
// agent_skill_installs joined to agents.
function useInUseAgents(_zoneId: string, _libraryId: string | undefined): {
  agents: InUseAgent[]; loading: boolean
} {
  return { agents: [], loading: false }
}

export function SkillsLibraryDetailModal({ open, zoneId, entry, onClose }: Props) {
  const [files, setFiles] = useState<SkillLibraryFileMeta[]>([])
  const [readme, setReadme] = useState<string>('')
  const [loading, setLoading] = useState(false)
  const { agents, loading: agentsLoading } = useInUseAgents(zoneId, entry?.id)

  useEffect(() => {
    if (!entry) return
    let cancelled = false
    setLoading(true); setReadme('')
    zoneSkillLibrary.get(zoneId, entry.id)
      .then(async (r) => {
        if (cancelled) return
        setFiles(r.files)
        const skillMd = r.files.find((f) => f.relPath === 'SKILL.md')
        if (skillMd) {
          const file = await zoneSkillLibrary.getFile(zoneId, entry.id, 'SKILL.md')
          if (!cancelled && !file.binary) setReadme(file.content)
        }
      })
      .finally(() => { if (!cancelled) setLoading(false) })
    return () => { cancelled = true }
  }, [zoneId, entry])

  if (!entry) return null

  return (
    <Modal open={open} onClose={onClose} title={entry.displayName || entry.name} size="lg">
      <div className="space-y-4">
        <div className="text-sm">
          <p>
            <span className="text-muted-foreground">Source:</span>{' '}
            <span className="font-mono break-all">{entry.sourceUrl}</span>
            {entry.sourceSubpath && <span className="font-mono"> /{entry.sourceSubpath}</span>}
          </p>
          <p className="text-xs text-muted-foreground">
            {entry.fileCount} files · {(entry.totalBytes / 1024).toFixed(1)} KB · imported {new Date(entry.importedAt).toLocaleString()}
          </p>
        </div>

        <section>
          <h4 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground mb-1">SKILL.md</h4>
          {loading ? (
            <div className="flex items-center gap-2 text-muted-foreground"><Loader2 className="h-4 w-4 animate-spin" /> Loading…</div>
          ) : readme ? (
            <pre className="text-xs whitespace-pre-wrap font-mono bg-muted rounded p-3 max-h-60 overflow-y-auto">{readme}</pre>
          ) : (
            <p className="text-xs text-muted-foreground">No SKILL.md found in this skill.</p>
          )}
        </section>

        <section>
          <h4 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground mb-1">Files ({files.length})</h4>
          <ul className="text-xs font-mono max-h-32 overflow-y-auto space-y-0.5">
            {files.map((f) => (
              <li key={f.relPath} className="text-muted-foreground">
                {f.relPath} <span className="opacity-60">({f.size} B)</span>
              </li>
            ))}
          </ul>
        </section>

        <section>
          <h4 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground mb-1">In use ({agents.length})</h4>
          {agentsLoading ? (
            <div className="flex items-center gap-2 text-muted-foreground"><Loader2 className="h-4 w-4 animate-spin" /> Loading…</div>
          ) : agents.length === 0 ? (
            <p className="text-xs text-muted-foreground">
              No agents are currently using this skill. (Phase 3 will let you install it to specific agents.)
            </p>
          ) : (
            <ul className="text-xs space-y-1">
              {agents.map((a) => (
                <li key={a.id} className="flex items-center justify-between">
                  <span>{a.name}</span>
                  <span className="text-muted-foreground">installed {new Date(a.installedAt).toLocaleDateString()}</span>
                </li>
              ))}
            </ul>
          )}
        </section>
      </div>
    </Modal>
  )
}

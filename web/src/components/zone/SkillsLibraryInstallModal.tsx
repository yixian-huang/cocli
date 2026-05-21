import { useEffect, useMemo, useState } from 'react'
import { Modal, Button, Badge } from '@/components/ui'
import { agents as agentsApi, agentSkills, zoneSkillLibrary } from '@/api/client'
import { useAgentSkillStore } from '@/stores/agentSkillStore'
import type { Agent, SkillLibraryEntry } from '@/lib/types'
import { Loader2, AlertTriangle, CheckCircle, XCircle } from 'lucide-react'

interface Props {
  zoneId: string
  presetAgentId?: string
  onClose: () => void
  onInstalled: () => void
}

interface RowResult {
  status: 'pending' | 'ok' | 'error'
  message?: string
}

export function SkillsLibraryInstallModal({ zoneId, presetAgentId, onClose, onInstalled }: Props) {
  const compat = useAgentSkillStore((s) => s.compatibility)
  const loadCompat = useAgentSkillStore((s) => s.loadCompatibility)

  const [library, setLibrary] = useState<SkillLibraryEntry[]>([])
  const [agents, setAgents] = useState<Agent[]>([])
  const [selectedLib, setSelectedLib] = useState<string | null>(null)
  const [selectedAgents, setSelectedAgents] = useState<Set<string>>(
    new Set(presetAgentId ? [presetAgentId] : []),
  )
  const [submitting, setSubmitting] = useState(false)
  const [results, setResults] = useState<Record<string, RowResult>>({})
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    loadCompat()
    Promise.all([
      zoneSkillLibrary.list(zoneId),
      agentsApi.list(zoneId),
    ])
      .then(([libRes, ag]) => {
        setLibrary(libRes.entries ?? [])
        setAgents(ag)
      })
      .finally(() => setLoading(false))
  }, [zoneId, loadCompat])

  const compatFor = (rt: string) => compat?.[rt] ?? 'unknown'
  const disabledFor = (rt: string) => compatFor(rt) === 'unsupported'

  const compatBadge = (rt: string) => {
    const c = compatFor(rt)
    if (c === 'supported') return <Badge variant="success" size="sm">✓</Badge>
    if (c === 'uncertain') return <Badge variant="warning" size="sm">⚠</Badge>
    return <Badge variant="error" size="sm">✗</Badge>
  }

  const toggleAgent = (id: string) => {
    setSelectedAgents((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id); else next.add(id)
      return next
    })
  }

  const visibleAgents = useMemo(() => agents.filter((a) => a.zoneId === zoneId), [agents, zoneId])

  const onSubmit = async () => {
    if (!selectedLib || selectedAgents.size === 0) return
    setSubmitting(true)
    const targets = Array.from(selectedAgents)
    setResults(Object.fromEntries(targets.map((id) => [id, { status: 'pending' as const }])))
    const settled = await Promise.allSettled(
      targets.map((id) => agentSkills.install(id, selectedLib).then(() => id)),
    )
    const next: Record<string, RowResult> = {}
    settled.forEach((r, i) => {
      const id = targets[i]
      if (r.status === 'fulfilled') {
        next[id] = { status: 'ok' }
      } else {
        next[id] = { status: 'error', message: (r.reason as Error).message }
      }
    })
    setResults(next)
    setSubmitting(false)
    const allOK = Object.values(next).every((r) => r.status === 'ok')
    if (allOK) onInstalled()
  }

  return (
    <Modal open onClose={onClose} title="Install Skill to Agents" size="lg">
      {loading ? (
        <Loader2 className="h-5 w-5 animate-spin" />
      ) : (
        <div className="space-y-3">
          <div>
            <h4 className="text-xs font-semibold mb-1">Skill</h4>
            <ul className="border rounded divide-y max-h-32 overflow-y-auto">
              {library.map((l) => (
                <li key={l.id}>
                  <button
                    className={`w-full text-left px-2 py-1 text-xs ${selectedLib === l.id ? 'bg-accent' : 'hover:bg-accent/50'}`}
                    onClick={() => setSelectedLib(l.id)}
                  >
                    {l.displayName || l.name}
                  </button>
                </li>
              ))}
            </ul>
          </div>

          <div>
            <h4 className="text-xs font-semibold mb-1">Agents</h4>
            <table className="w-full text-xs border rounded">
              <thead className="bg-muted">
                <tr>
                  <th className="text-left px-2 py-1">Select</th>
                  <th className="text-left px-2 py-1">Name</th>
                  <th className="text-left px-2 py-1">Runtime</th>
                  <th className="text-left px-2 py-1">Compat</th>
                  <th className="text-left px-2 py-1">Result</th>
                </tr>
              </thead>
              <tbody>
                {visibleAgents.map((a) => (
                  <tr key={a.id} className={disabledFor(a.runtime) ? 'opacity-50' : ''}>
                    <td className="px-2 py-1">
                      <input
                        type="checkbox"
                        aria-label={a.name}
                        disabled={disabledFor(a.runtime)}
                        checked={selectedAgents.has(a.id)}
                        onChange={() => toggleAgent(a.id)}
                      />
                    </td>
                    <td className="px-2 py-1">{a.name}</td>
                    <td className="px-2 py-1 font-mono">{a.runtime}</td>
                    <td className="px-2 py-1">{compatBadge(a.runtime)}</td>
                    <td className="px-2 py-1">
                      {results[a.id]?.status === 'pending' && <Loader2 className="h-3 w-3 animate-spin" />}
                      {results[a.id]?.status === 'ok' && <CheckCircle className="h-3 w-3 text-emerald-500" />}
                      {results[a.id]?.status === 'error' && (
                        <span className="text-destructive flex items-center gap-1">
                          <XCircle className="h-3 w-3" /> {results[a.id]?.message}
                        </span>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          <div className="flex justify-end gap-2 pt-2 border-t">
            <Button variant="ghost" onClick={onClose}>Cancel</Button>
            <Button
              onClick={onSubmit}
              disabled={!selectedLib || selectedAgents.size === 0 || submitting}
              aria-label="Install"
            >
              {submitting ? <><Loader2 className="h-3 w-3 mr-1 animate-spin" /> Installing</> : 'Install'}
            </Button>
          </div>
          {!compat && (
            <p className="text-[10px] text-muted-foreground flex items-center gap-1">
              <AlertTriangle className="h-3 w-3" /> Compatibility matrix unavailable
            </p>
          )}
        </div>
      )}
    </Modal>
  )
}

import { useState, useEffect, useMemo } from 'react'
import { Badge, Button } from '@/components/ui'
import { Loader2, AlertCircle, Sparkles, Globe, FolderCode, Search, RefreshCw, Plus, Eye, Trash2 } from 'lucide-react'
import { useAgentSkillStore } from '@/stores/agentSkillStore'
import { useAgentStore } from '@/stores/agentStore'
import { toast, toastError } from '@/stores/toastStore'
import type { SkillView } from '@/lib/types'
import { SkillViewModal } from './SkillViewModal'
import { SkillsLibraryInstallModal } from '../zone/SkillsLibraryInstallModal'

export function SkillsTab({ agentId, offline }: { agentId: string; offline: boolean }) {
  const agent = useAgentStore((s) => s.agents.find((a) => a.id === agentId))
  const skills = useAgentSkillStore((s) => s.skillsByAgent[agentId] || [])
  const loading = useAgentSkillStore((s) => s.loadingByAgent[agentId] ?? true)
  const err = useAgentSkillStore((s) => s.errorByAgent[agentId])
  const compat = useAgentSkillStore((s) => s.compatibility)
  const fetchForAgent = useAgentSkillStore((s) => s.fetchForAgent)
  const loadCompat = useAgentSkillStore((s) => s.loadCompatibility)
  const uninstall = useAgentSkillStore((s) => s.uninstall)

  const [search, setSearch] = useState('')
  const [viewing, setViewing] = useState<SkillView | null>(null)
  const [showInstall, setShowInstall] = useState(false)

  useEffect(() => {
    if (offline) return
    fetchForAgent(agentId)
    loadCompat()
  }, [agentId, offline, fetchForAgent, loadCompat])

  const runtimeCompat = agent ? compat?.[agent.runtime] : undefined
  const installDisabled = !agent || runtimeCompat === 'unsupported'

  const filtered = useMemo(() => {
    if (!search.trim()) return skills
    const q = search.toLowerCase()
    return skills.filter(
      (s) =>
        s.name.toLowerCase().includes(q) ||
        (s.displayName ?? '').toLowerCase().includes(q) ||
        (s.description ?? '').toLowerCase().includes(q),
    )
  }, [skills, search])

  if (offline || err) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center text-muted-foreground gap-2">
        <AlertCircle className="h-8 w-8 opacity-40" />
        <span className="text-sm">
          {offline ? 'Agent offline, skills unavailable' : `Error: ${err}`}
        </span>
      </div>
    )
  }

  if (loading) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
      </div>
    )
  }

  const globalSkills = filtered.filter((s) => s.type === 'global')
  const workspaceSkills = filtered.filter((s) => s.type === 'workspace')

  const handleUninstall = async (skill: SkillView) => {
    if (!skill.installId) return
    try {
      await uninstall(agentId, skill.installId)
      toast(`Uninstalled ${skill.name}`, 'info')
    } catch (e) {
      toastError(`Uninstall failed: ${(e as Error).message}`)
    }
  }

  const stateBadge = (state: SkillView['state']) => {
    if (state === 'managed') return <Badge variant="success" size="sm">managed</Badge>
    if (state === 'external') return <Badge variant="warning" size="sm">external</Badge>
    return <Badge variant="error" size="sm">broken</Badge>
  }

  const renderRow = (s: SkillView) => (
    <div key={`${s.type}/${s.name}/${s.installPath ?? ''}`} className="rounded-lg border p-3 space-y-1">
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          <Sparkles className="h-3.5 w-3.5 text-primary" />
          <span className="text-sm font-medium">{s.displayName || s.name}</span>
          {s.userInvocable && <Badge variant="info" size="sm">/{s.name}</Badge>}
          {stateBadge(s.state)}
        </div>
        <div className="flex gap-1">
          {s.state !== 'broken' && (
            <Button size="sm" variant="ghost" onClick={() => setViewing(s)} aria-label="View">
              <Eye className="h-3.5 w-3.5" />
            </Button>
          )}
          {(s.state === 'managed' || s.state === 'broken') && s.installId && (
            <Button
              size="sm"
              variant="ghost"
              onClick={() => handleUninstall(s)}
              aria-label={s.state === 'broken' ? 'Remove record' : 'Uninstall'}
            >
              <Trash2 className="h-3.5 w-3.5" />
            </Button>
          )}
        </div>
      </div>
      {s.description && <p className="text-xs text-muted-foreground">{s.description}</p>}
      {s.state === 'broken' ? (
        <p className="text-[10px] text-destructive font-mono">missing on disk: {s.installPath}</p>
      ) : (
        <p className="text-[10px] text-muted-foreground/60 font-mono truncate">{s.path}</p>
      )}
      {s.state === 'managed' && s.sourceUrl && (
        <p className="text-[10px] text-muted-foreground/60 truncate">
          from {s.sourceUrl} {s.sourceRef && `@ ${s.sourceRef.slice(0, 7)}`}
        </p>
      )}
    </div>
  )

  const section = (title: string, items: SkillView[], icon: React.ReactNode) => (
    <div className="space-y-2">
      <h4 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider flex items-center gap-1.5">
        {icon} {title} ({items.length})
      </h4>
      {items.length === 0 ? (
        <p className="text-xs text-muted-foreground">No skills found</p>
      ) : (
        <div className="grid gap-2">{items.map(renderRow)}</div>
      )}
    </div>
  )

  return (
    <div className="flex-1 overflow-y-auto p-4 space-y-4">
      <div className="flex items-center gap-2">
        <div className="relative flex-1">
          <Search className="absolute left-2 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground" />
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search skills"
            className="w-full pl-7 pr-2 py-1 text-xs border rounded bg-background"
          />
        </div>
        <Button size="sm" variant="ghost" onClick={() => fetchForAgent(agentId)} aria-label="Refetch">
          <RefreshCw className="h-3.5 w-3.5" />
        </Button>
        <Button
          size="sm"
          disabled={installDisabled}
          onClick={() => setShowInstall(true)}
          aria-label="Install from Library"
          title={installDisabled ? `${agent?.runtime ?? 'this runtime'} does not support skills` : 'Install from library'}
        >
          <Plus className="h-3.5 w-3.5 mr-1" /> Install from Library
        </Button>
      </div>
      {section('Global Skills', globalSkills, <Globe className="h-3 w-3" />)}
      {section('Workspace Skills', workspaceSkills, <FolderCode className="h-3 w-3" />)}

      {viewing && agent && (
        <SkillViewModal
          agentId={agentId}
          skill={viewing}
          onClose={() => setViewing(null)}
        />
      )}
      {showInstall && agent && (
        <SkillsLibraryInstallModal
          zoneId={agent.zoneId}
          presetAgentId={agentId}
          onClose={() => setShowInstall(false)}
          onInstalled={() => {
            setShowInstall(false)
            fetchForAgent(agentId)
          }}
        />
      )}
    </div>
  )
}

import { useEffect, useState } from 'react'
import { Link, useParams } from 'react-router-dom'
import { ArrowLeft, Trash2 } from 'lucide-react'
import { Skeleton, Card, Button, ConfirmDialog } from '@/components/ui'
import {
  daemons as daemonsApi,
  agents as agentsApi,
} from '@/api/client'
import { useZoneStore } from '@/stores/zoneStore'
import { useMachineStatusStore } from '@/stores/machineStatusStore'
import { useNavigate } from 'react-router-dom'
import { toast, toastError } from '@/stores/toastStore'
import { VersionStatusBadge } from '@/components/agents/VersionStatusBadge'
import {
  DaemonInstallCommands,
  UpgradeDaemonButton,
} from '@/components/daemons/daemonWidgets'
import type { Agent, Machine } from '@/lib/types'
import { agentPath, daemonsPath } from '@/lib/paths'

export function DaemonDetailRoute() {
  const { machineId } = useParams<{ machineId: string }>()
  const navigate = useNavigate()
  const zoneId = useZoneStore((s) => s.activeZoneId)
  const zoneSlug = useZoneStore((s) => s.activeZoneSlug)
  const overlay = useMachineStatusStore((s) =>
    machineId ? s.overlay[machineId] : undefined,
  )

  const [machine, setMachine] = useState<(Machine & { connected: boolean }) | null>(null)
  const [loading, setLoading] = useState(true)
  const [agentsOnDaemon, setAgentsOnDaemon] = useState<Agent[]>([])
  const [upgrading, setUpgrading] = useState(false)
  const [copied, setCopied] = useState(false)
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false)
  const [deleting, setDeleting] = useState(false)

  useEffect(() => {
    if (!machineId || !zoneId) return
    let active = true
    setLoading(true)
    Promise.all([
      daemonsApi.list(zoneId),
      agentsApi.list(zoneId),
    ])
      .then(([machines, agents]) => {
        if (!active) return
        setMachine(machines.find((m) => m.id === machineId) ?? null)
        setAgentsOnDaemon(agents.filter((a) => a.machineId === machineId))
      })
      .catch(() => {})
      .finally(() => {
        if (active) setLoading(false)
      })
    return () => {
      active = false
    }
  }, [machineId, zoneId])

  const handleUpgrade = async () => {
    if (!zoneId || !machineId) return
    setUpgrading(true)
    try {
      await daemonsApi.upgrade(zoneId, machineId)
      toast('Upgrade dispatched — daemon will reconnect with the new version', 'success')
    } catch (err) {
      toastError(err instanceof Error ? err.message : 'Failed to dispatch upgrade')
    } finally {
      setTimeout(() => setUpgrading(false), 5000)
    }
  }

  const handleDelete = async () => {
    if (!zoneId || !machineId) return
    setDeleting(true)
    try {
      await daemonsApi.remove(zoneId, machineId)
      toast('Daemon removed', 'success')
      setDeleteDialogOpen(false)
      navigate(daemonsPath({ zoneSlug }))
    } catch (err) {
      toastError(err instanceof Error ? err.message : 'Failed to remove daemon')
    } finally {
      setDeleting(false)
    }
  }

  const handleCopy = async (text: string) => {
    await navigator.clipboard.writeText(text)
    setCopied(true)
    toast('Copied to clipboard', 'success')
    setTimeout(() => setCopied(false), 2000)
  }

  if (loading || !machine) {
    return (
      <div className="p-6">
        <Skeleton variant="rectangle" height="200px" width="100%" />
      </div>
    )
  }

  const env = machine.environment
  const status = overlay?.versionStatus ?? machine.versionStatus
  const daemonVersion = overlay?.daemonVersion ?? machine.daemonVersion

  return (
    <div className="p-4 space-y-4 max-w-4xl mx-auto">
      <div className="flex items-center gap-3">
        <Link to={daemonsPath({ zoneSlug })} className="text-sm text-muted-foreground hover:text-foreground">
          <ArrowLeft className="inline h-4 w-4 mr-1" />Back to daemons
        </Link>
        <h1 className="text-lg font-semibold ml-auto">{machine.hostname || machine.id.slice(0, 12)}</h1>
        <VersionStatusBadge
          machineId={machine.id}
          initialStatus={machine.versionStatus}
          initialDaemonVersion={machine.daemonVersion}
        />
        <UpgradeDaemonButton
          machineId={machine.id}
          initialStatus={machine.versionStatus}
          connected={!!machine.connected}
          busy={upgrading}
          onUpgrade={handleUpgrade}
        />
        <Button
          variant="ghost"
          size="sm"
          onClick={() => setDeleteDialogOpen(true)}
          className="text-red-500 hover:bg-red-500/10 gap-1"
          title="Remove daemon"
        >
          <Trash2 className="h-3 w-3" /> Delete
        </Button>
      </div>

      <ConfirmDialog
        open={deleteDialogOpen}
        onClose={() => setDeleteDialogOpen(false)}
        onConfirm={handleDelete}
        title="Remove this daemon?"
        message="This removes the daemon record from the zone and unbinds agents on this machine. Running daemons should be stopped first. This cannot be undone."
        confirmLabel="Delete"
        variant="danger"
        loading={deleting}
      />

      <Card>
        <div className="p-4 grid grid-cols-2 sm:grid-cols-3 gap-4 text-sm">
          <Field label="machineID" value={machine.id} />
          <Field label="status" value={machine.connected ? 'online' : 'offline'} />
          <Field label="version" value={daemonVersion ?? '—'} />
          <Field label="versionStatus" value={status} />
          <Field label="last seen" value={machine.lastSeen ?? '—'} />
          <Field label="IP" value={machine.lastIp ?? '—'} />
          <Field label="OS" value={machine.os ?? '—'} />
          <Field label="CPU" value={env?.cpu ?? '—'} />
          <Field label="memory" value={env?.memory ?? '—'} />
          <Field label="disk free" value={env?.disk_free ?? '—'} />
        </div>
      </Card>

      {(machine.runtimes?.length ?? 0) > 0 && (
        <Card>
          <div className="p-4">
            <h2 className="text-sm font-semibold mb-2">Runtimes</h2>
            <div className="flex flex-wrap gap-1">
              {machine.runtimes?.map((r) => (
                <span
                  key={r}
                  className="inline-flex items-center px-1.5 py-0.5 rounded bg-primary/10 text-primary text-[11px] font-medium"
                >
                  {r}
                </span>
              ))}
            </div>
          </div>
        </Card>
      )}

      {(env?.languages?.length || env?.tools?.length) ? (
        <Card>
          <div className="p-4">
            <h2 className="text-sm font-semibold mb-2">Tools &amp; languages</h2>
            <div className="flex flex-wrap gap-1">
              {env?.languages?.map((l) => (
                <span key={l} className="px-1.5 py-0.5 rounded bg-muted text-muted-foreground text-[11px]">{l}</span>
              ))}
              {env?.tools?.map((t) => (
                <span key={t} className="px-1.5 py-0.5 rounded bg-muted text-muted-foreground text-[11px]">{t}</span>
              ))}
            </div>
          </div>
        </Card>
      ) : null}

      <Card>
        <div className="p-4">
          <h2 className="text-sm font-semibold mb-2">Agents on this daemon</h2>
          {agentsOnDaemon.length === 0 ? (
            <div className="text-sm text-muted-foreground">No agents.</div>
          ) : (
            <ul className="space-y-1 text-sm">
              {agentsOnDaemon.map((a) => (
                <li key={a.id}>
                  • <Link to={agentPath({ zoneSlug, agentId: a.id })} className="text-primary hover:underline">@{a.name}</Link>{' '}
                  <span className="text-muted-foreground text-xs">{a.status}</span>
                </li>
              ))}
            </ul>
          )}
        </div>
      </Card>

      <Card>
        <div className="p-4">
          <h2 className="text-sm font-semibold mb-2">Install / reconnect</h2>
          {zoneId ? (
            <DaemonInstallCommands
              zoneId={zoneId}
              machineId={machine.id}
              connected={!!machine.connected}
              copied={copied}
              onCopy={handleCopy}
            />
          ) : null}
        </div>
      </Card>

      <Card>
        <div className="p-4 text-sm text-muted-foreground">Metrics — coming next round.</div>
      </Card>
    </div>
  )
}

function Field({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="text-[11px] text-muted-foreground">{label}</div>
      <div className="font-mono text-xs break-all">{value}</div>
    </div>
  )
}

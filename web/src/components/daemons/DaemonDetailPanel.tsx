import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Trash2 } from 'lucide-react'
import { agents as agentsApi, daemons as daemonsApi } from '@/api/client'
import { useZoneStore } from '@/stores/zoneStore'
import { toast, toastError } from '@/stores/toastStore'
import { Button, Skeleton, ConfirmDialog } from '@/components/ui'
import { DaemonInstallCommands, UpgradeDaemonButton } from '@/components/daemons/daemonWidgets'
import type { Agent, Machine } from '@/lib/types'
import { formatLastConnection, formatShortDateTime } from '@/lib/formatTime'
import { agentPath, daemonDetailPath } from '@/lib/paths'
import { Link } from 'react-router-dom'
import { agentStatusLabel } from '@/lib/status'

type DaemonMachine = Machine & { connected: boolean }

function DetailSection({
  title,
  children,
  className,
}: {
  title: string
  children: React.ReactNode
  className?: string
}) {
  return (
    <section className={className}>
      <h4 className="text-sm font-medium uppercase tracking-wider text-content-secondary mb-2">
        {title}
      </h4>
      {children}
    </section>
  )
}

function MetaItem({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="text-sm text-content-secondary">{label}</div>
      <div className="text-sm font-medium text-foreground tabular-nums">{value}</div>
    </div>
  )
}

function DaemonAgentRow({
  agent,
  zoneSlug,
  machineId,
}: {
  agent: Agent
  zoneSlug: string | null | undefined
  machineId: string
}) {
  const returnTo = zoneSlug ? daemonDetailPath({ zoneSlug, machineId }) : undefined

  return (
    <li className="px-3 py-2.5 hover:bg-accent/10 transition-colors">
      <div className="flex items-center gap-2 min-w-0">
        <Link
          to={agentPath({ zoneSlug, agentId: agent.id })}
          state={returnTo ? { returnTo } : undefined}
          className="text-primary hover:underline font-medium text-sm truncate"
        >
          @{agent.name}
        </Link>
        <span className="text-sm text-content-secondary ml-auto shrink-0">
          {agentStatusLabel(agent.status)}
        </span>
      </div>
      <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-sm text-content-secondary">
        <span className="font-mono px-1 py-0.5 rounded bg-muted/80 text-foreground/80">
          {agent.runtime}
        </span>
        <span className="truncate max-w-48" title={agent.model}>
          {agent.model}
        </span>
        <span className="tabular-nums">{formatShortDateTime(agent.createdAt)}</span>
      </div>
    </li>
  )
}

export function DaemonDetailPanel({
  machine,
  onDeleted,
}: {
  machine: DaemonMachine
  onDeleted?: () => void
}) {
  const { t } = useTranslation()
  const zoneId = useZoneStore((s) => s.activeZoneId)
  const zoneSlug = useZoneStore((s) => s.activeZoneSlug)

  const [agentsOnDaemon, setAgentsOnDaemon] = useState<Agent[]>([])
  const [agentsLoading, setAgentsLoading] = useState(true)
  const [upgrading, setUpgrading] = useState(false)
  const [copied, setCopied] = useState(false)
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false)
  const [deleting, setDeleting] = useState(false)

  useEffect(() => {
    if (!zoneId) return
    let active = true
    setAgentsLoading(true)
    agentsApi
      .list(zoneId)
      .then((agents) => {
        if (active) setAgentsOnDaemon(agents.filter((a) => a.machineId === machine.id))
      })
      .catch(() => {})
      .finally(() => {
        if (active) setAgentsLoading(false)
      })
    return () => {
      active = false
    }
  }, [machine.id, zoneId])

  const handleUpgrade = async () => {
    if (!zoneId) return
    setUpgrading(true)
    try {
      await daemonsApi.upgrade(zoneId, machine.id)
      toast(t('daemon.upgradeDispatched'), 'success')
    } catch (err) {
      toastError(err instanceof Error ? err.message : t('daemon.upgradeFailed'))
    } finally {
      setTimeout(() => setUpgrading(false), 5000)
    }
  }

  const handleDelete = async () => {
    if (!zoneId) return
    setDeleting(true)
    try {
      await daemonsApi.remove(zoneId, machine.id)
      toast(t('daemon.removed'), 'success')
      setDeleteDialogOpen(false)
      onDeleted?.()
    } catch (err) {
      toastError(err instanceof Error ? err.message : t('daemon.removeFailed'))
    } finally {
      setDeleting(false)
    }
  }

  const handleCopy = async (text: string) => {
    await navigator.clipboard.writeText(text)
    setCopied(true)
    toast(t('common.copied'), 'success')
    setTimeout(() => setCopied(false), 2000)
  }

  const env = machine.environment
  const connected = !!machine.connected

  return (
    <div className="border-t border-border/70 bg-muted/15 px-4 py-4 space-y-4">
      <div className="flex flex-wrap items-center gap-2">
        <div className="flex-1" />
        <UpgradeDaemonButton
          machineId={machine.id}
          initialStatus={machine.versionStatus}
          connected={connected}
          busy={upgrading}
          onUpgrade={handleUpgrade}
        />
        <Button
          variant="ghost"
          size="sm"
          onClick={() => setDeleteDialogOpen(true)}
          className="text-red-500 hover:bg-red-500/10 gap-1 h-7"
          title={t('daemon.removeTitle')}
        >
          <Trash2 className="h-3.5 w-3.5" /> {t('common.delete')}
        </Button>
      </div>

      <ConfirmDialog
        open={deleteDialogOpen}
        onClose={() => setDeleteDialogOpen(false)}
        onConfirm={handleDelete}
        title={t('daemon.removeConfirmTitle')}
        message={t('daemon.removeConfirmMessage')}
        confirmLabel={t('common.delete')}
        variant="danger"
        loading={deleting}
      />

      <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 rounded-md bg-background/60 border border-border/50 px-3 py-2.5">
        <MetaItem label={t('daemon.meta.created')} value={formatShortDateTime(machine.createdAt)} />
        <MetaItem
          label={t('daemon.meta.lastConnection')}
          value={formatLastConnection(machine.lastSeen, connected)}
        />
        <MetaItem label={t('daemon.meta.ip')} value={machine.lastIp ?? '—'} />
        <MetaItem label={t('daemon.meta.os')} value={machine.os ?? '—'} />
      </div>

      {(env?.tools?.length || env?.languages?.length) ? (
        <DetailSection title={t('daemon.environment')}>
          <div className="flex flex-wrap gap-1">
            {env?.tools?.map((t) => (
              <span
                key={t}
                className="px-2 py-0.5 rounded-md bg-background border border-border/50 text-content-secondary text-sm"
              >
                {t}
              </span>
            ))}
            {env?.languages?.map((l) => (
              <span
                key={l}
                className="px-2 py-0.5 rounded-md bg-background border border-border/50 text-content-secondary text-sm"
              >
                {l}
              </span>
            ))}
          </div>
        </DetailSection>
      ) : null}

      <DetailSection title={t('daemon.agents')}>
        {agentsLoading ? (
          <Skeleton variant="rectangle" height="48px" width="100%" />
        ) : agentsOnDaemon.length === 0 ? (
          <p className="text-sm text-content-secondary">{t('daemon.noAgents')}</p>
        ) : (
          <ul className="rounded-md bg-background/60 border border-border/50 divide-y divide-border/50 overflow-hidden">
            {agentsOnDaemon.map((a) => (
              <DaemonAgentRow
                key={a.id}
                agent={a}
                zoneSlug={zoneSlug}
                machineId={machine.id}
              />
            ))}
          </ul>
        )}
      </DetailSection>

      <DetailSection title={t('daemon.install')}>
        {zoneId ? (
          <DaemonInstallCommands
            zoneId={zoneId}
            machineId={machine.id}
            connected={connected}
            copied={copied}
            onCopy={handleCopy}
          />
        ) : null}
      </DetailSection>
    </div>
  )
}

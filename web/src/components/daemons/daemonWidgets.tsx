import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { i18n } from '@/i18n'
import { daemons as daemonsApi } from '@/api/client'
import { useMachineStatusStore } from '@/stores/machineStatusStore'
import { toast, toastError } from '@/stores/toastStore'
import type { Machine } from '@/lib/types'
import { Check, Copy, Loader2, ArrowUpCircle } from 'lucide-react'

export async function dispatchDaemonUpgrade(zoneId: string, machineId: string) {
  try {
    await daemonsApi.upgrade(zoneId, machineId)
    toast(i18n.t('daemon.upgradeDispatched'), 'success')
  } catch (err) {
    toastError(err instanceof Error ? err.message : i18n.t('daemon.upgradeFailed'))
  }
}

export async function deleteDaemon(zoneId: string, machineId: string) {
  try {
    await daemonsApi.remove(zoneId, machineId)
    toast(i18n.t('daemon.removed'), 'success')
  } catch (err) {
    toastError(err instanceof Error ? err.message : i18n.t('daemon.removeFailed'))
  }
}

export function InstallCommand({ installCommand, onCopy, copied }: {
  installCommand: string
  onCopy: (text: string) => void
  copied: boolean
}) {
  const { t } = useTranslation()
  return (
    <div className="space-y-3">
      <p className="text-sm text-content-secondary">
        {t('daemon.installCopyHint')}
      </p>
      <div className="relative">
        <pre className="text-sm font-mono bg-muted rounded-lg p-3 pr-10 overflow-x-auto whitespace-pre-wrap break-all">
          {installCommand}
        </pre>
        <button
          type="button"
          onClick={() => onCopy(installCommand)}
          className="absolute top-2 right-2 p-1.5 rounded hover:bg-accent transition-colors"
          title={t('daemon.copyInstallCommand')}
        >
          {copied ? <Check className="h-3.5 w-3.5 text-green-500" /> : <Copy className="h-3.5 w-3.5 text-muted-foreground" />}
        </button>
      </div>
    </div>
  )
}

export function DaemonInstallCommands({
  zoneId,
  machineId,
  connected,
  copied,
  onCopy,
}: {
  zoneId: string
  machineId: string
  connected: boolean
  copied: boolean
  onCopy: (text: string) => void
}) {
  const { t } = useTranslation()
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [installCommand, setInstallCommand] = useState<string | null>(null)

  const load = () => {
    setLoading(true)
    setError(null)
    return daemonsApi
      .installCommands(zoneId, machineId)
      .then((res) => setInstallCommand(res.installCommand))
      .catch((err) => {
        setError(err instanceof Error ? err.message : t('daemon.loadInstallFailed'))
      })
      .finally(() => setLoading(false))
  }

  useEffect(() => {
    if (!zoneId || !machineId) return
    let active = true
    setLoading(true)
    setError(null)
    daemonsApi
      .installCommands(zoneId, machineId)
      .then((res) => {
        if (active) setInstallCommand(res.installCommand)
      })
      .catch((err) => {
        if (active) {
          setError(err instanceof Error ? err.message : t('daemon.loadInstallFailed'))
        }
      })
      .finally(() => {
        if (active) setLoading(false)
      })
    return () => {
      active = false
    }
  }, [zoneId, machineId, t])

  if (loading) {
    return <div className="text-sm text-content-secondary">{t('daemon.loadingInstall')}</div>
  }

  if (error) {
    return (
      <div className="text-sm text-destructive">
        {error}{' '}
        <button type="button" className="text-primary hover:underline" onClick={load}>
          {t('common.retry')}
        </button>
      </div>
    )
  }

  if (!installCommand) return null

  return (
    <div className="space-y-3">
      {connected ? (
        <p className="text-sm rounded-lg border border-warning/40 bg-warning/10 text-warning px-3 py-2">
          <strong>{t('daemon.onlineWarningStrong')}</strong> {t('daemon.onlineWarningBody')}
        </p>
      ) : null}
      <InstallCommand installCommand={installCommand} onCopy={onCopy} copied={copied} />
    </div>
  )
}

export function UpgradeDaemonButton({
  machineId,
  initialStatus,
  connected,
  busy,
  onUpgrade,
}: {
  machineId: string
  initialStatus: Machine['versionStatus']
  connected: boolean
  busy: boolean
  onUpgrade: () => void
}) {
  const { t } = useTranslation()
  const overlay = useMachineStatusStore((s) => s.overlay[machineId])
  const status = overlay?.versionStatus ?? initialStatus
  if (status !== 'outdated') return null

  const disabled = !connected || busy
  const title = !connected
    ? t('daemon.upgradeOffline')
    : busy
      ? t('daemon.upgradeInProgress')
      : t('daemon.upgradeTooltip')

  return (
    <button
      onClick={onUpgrade}
      disabled={disabled}
      title={title}
      className="flex items-center gap-1 px-2 py-1 rounded text-sm text-warning hover:bg-warning/10 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
    >
      {busy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <ArrowUpCircle className="h-3.5 w-3.5" />}
      {t('daemon.upgrade')}
    </button>
  )
}

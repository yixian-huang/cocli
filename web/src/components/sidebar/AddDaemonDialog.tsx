import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Modal, Button } from '@/components/ui'
import { useDialogStore } from '@/stores/dialogStore'
import { daemons as daemonsApi } from '@/api/client'
import { toastError } from '@/stores/toastStore'
import { InstallCommand } from '@/components/daemons/daemonWidgets'

export function AddDaemonDialog() {
  const { t } = useTranslation()
  const open = useDialogStore((s) => s.active === 'addDaemon')
  const payload = useDialogStore((s) => s.payload)
  const close = useDialogStore((s) => s.close)
  const zoneId = (payload as { zoneId?: string } | null)?.zoneId

  const [submitting, setSubmitting] = useState(false)
  const [installCommand, setInstallCommand] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)

  useEffect(() => {
    if (!open) {
      setSubmitting(false)
      setInstallCommand(null)
      setError(null)
      setCopied(false)
    }
  }, [open])

  const create = async () => {
    if (!zoneId) {
      setError(t('daemon.noActiveZone'))
      return
    }
    setSubmitting(true)
    setError(null)
    try {
      const res = await daemonsApi.create(zoneId)
      setInstallCommand(res.installCommand)
    } catch (err) {
      const msg = err instanceof Error ? err.message : t('daemon.registerFailed')
      setError(msg)
      toastError(msg)
    } finally {
      setSubmitting(false)
    }
  }

  const handleCopy = async (text: string) => {
    await navigator.clipboard.writeText(text)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  return (
    <Modal
      open={open}
      onClose={close}
      title={t('daemon.addTitle')}
      size="lg"
      footer={
        installCommand ? (
          <Button onClick={close}>{t('common.done')}</Button>
        ) : (
          <>
            <Button variant="ghost" onClick={close} disabled={submitting}>{t('common.cancel')}</Button>
            <Button onClick={create} disabled={submitting}>
              {submitting ? t('daemon.creating') : t('daemon.createShowCommand')}
            </Button>
          </>
        )
      }
    >
      {!installCommand && (
        <p className="text-sm text-content-secondary">
          {t('daemon.addDesc')}
        </p>
      )}
      {installCommand && (
        <InstallCommand installCommand={installCommand} onCopy={handleCopy} copied={copied} />
      )}
      {error && <div className="mt-2 text-sm text-destructive">{error}</div>}
    </Modal>
  )
}

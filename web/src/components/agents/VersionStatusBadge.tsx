import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { Badge } from '@/components/ui/Badge'
import { Tooltip } from '@/components/ui/Tooltip'
import { useMachineStatusStore } from '@/stores/machineStatusStore'
import { cn } from '@/lib/utils'
import type { MachineVersionStatus } from '@/lib/types'

interface Props {
  machineId: string
  initialStatus: MachineVersionStatus
  initialDaemonVersion?: string
  size?: 'sm' | 'md'
  className?: string
}

export function VersionStatusBadge({
  machineId,
  initialStatus,
  initialDaemonVersion,
  size = 'sm',
  className,
}: Props) {
  const { t } = useTranslation()
  const overlay = useMachineStatusStore((s) => s.overlay[machineId])
  const status = overlay?.versionStatus ?? initialStatus
  const daemonVersion = overlay?.daemonVersion ?? initialDaemonVersion

  if (status === 'current') return null

  const outdatedTooltip = t('version.outdatedTooltip')
  const unstampedTooltip = t('version.unstampedTooltip')

  const wrap = (badge: ReactNode, tip: string) => (
    <span className={cn('inline-flex', className)}>
      <Tooltip content={tip} delay={200}>
        {badge}
      </Tooltip>
    </span>
  )

  if (status === 'outdated') {
    const tip = daemonVersion
      ? t('version.outdatedWithVersion', { version: daemonVersion, tooltip: outdatedTooltip })
      : outdatedTooltip
    return wrap(
      <Badge variant="warning" size={size}>
        {t('version.outdated')}
      </Badge>,
      tip,
    )
  }

  return wrap(
    <Badge variant="default" size={size}>
      {t('version.unstamped')}
    </Badge>,
    unstampedTooltip,
  )
}

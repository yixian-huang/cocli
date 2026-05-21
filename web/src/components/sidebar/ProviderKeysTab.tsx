import { useState, useEffect } from 'react'
import { Button, Badge, EmptyState, Skeleton, ConfirmDialog } from '@/components/ui'
import { useZoneStore } from '@/stores/zoneStore'
import { useUserStore } from '@/stores/userStore'
import { useCocliCredentialsStore } from '@/stores/chatrsCredentialsStore'
import { toastError } from '@/stores/toastStore'
import { CreateKeyDialog, type ProfileOption } from './CreateKeyDialog'
import type { CreateCredentialInput } from '@/lib/types'
import { useTranslation } from 'react-i18next'

const PROFILES: ProfileOption[] = [
  { value: 'anthropic', label: 'Anthropic', requiresBaseUrl: false },
  { value: 'openai', label: 'OpenAI', requiresBaseUrl: false },
  { value: 'deepseek', label: 'DeepSeek', requiresBaseUrl: false },
  { value: 'kimi', label: 'Kimi (Moonshot)', requiresBaseUrl: false },
  { value: 'glm', label: 'GLM (Z.ai)', requiresBaseUrl: false },
  { value: 'qwen', label: 'Qwen', requiresBaseUrl: false },
  { value: 'openai_compat_custom', label: 'Custom (OpenAI-compat)', requiresBaseUrl: true },
]

function profileLabel(name: string): string {
  return PROFILES.find((p) => p.value === name)?.label ?? name
}

function formatRelativeTime(dateStr: string | undefined) {
  // 这里的相对时间展示先做最小迁移；后续可替换为 Intl.RelativeTimeFormat
  if (!dateStr) return 'Never'
  const date = new Date(dateStr)
  const now = new Date()
  const seconds = Math.floor((now.getTime() - date.getTime()) / 1000)

  if (seconds < 60) return 'Just now'
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`
  if (seconds < 604800) return `${Math.floor(seconds / 86400)}d ago`
  return `${Math.floor(seconds / 604800)}w ago`
}

export function ProviderKeysTab() {
  const { t } = useTranslation()
  const { activeZoneId } = useZoneStore()
  const { user } = useUserStore()
  const { byZone, loadingByZone, errorByZone, fetch, create, remove } = useCocliCredentialsStore()
  const [createDialogOpen, setCreateDialogOpen] = useState(false)
  const [deleteConfirm, setDeleteConfirm] = useState<{ open: boolean; name?: string }>({ open: false })
  const [deleting, setDeleting] = useState(false)

  const isAdmin = user?.role === 'admin'
  const keys = byZone[activeZoneId ?? ''] ?? []
  const loading = loadingByZone[activeZoneId ?? ''] ?? false
  const error = errorByZone[activeZoneId ?? ''] ?? null

  useEffect(() => {
    if (isAdmin && activeZoneId) {
      fetch(activeZoneId).catch((err) => {
        toastError(
          t('providerKeys.errors.loadFailed', { error: err instanceof Error ? err.message : t('common.unknownError') }),
        )
      })
    }
  }, [isAdmin, activeZoneId, fetch, t])

  if (!isAdmin) {
    return <EmptyState title={t('providerKeys.adminOnlyTitle')} description={t('providerKeys.adminOnlyDesc')} />
  }

  if (loading) {
    return (
      <div className="space-y-3 p-4">
        <Skeleton className="h-10 w-full" />
        <Skeleton className="h-10 w-full" />
        <Skeleton className="h-10 w-full" />
      </div>
    )
  }

  if (error) {
    return (
      <EmptyState
        title={t('providerKeys.errorLoadingTitle')}
        description={error}
        action={
          <Button
            onClick={() => {
              if (activeZoneId) fetch(activeZoneId)
            }}
          >
            {t('common.retry')}
          </Button>
        }
      />
    )
  }

  const handleCreateKey = async (input: CreateCredentialInput) => {
    try {
      if (!activeZoneId) throw new Error(t('providerKeys.errors.noZoneSelected'))
      await create(activeZoneId, input)
    } catch (err) {
      toastError(
        t('providerKeys.errors.createFailed', { error: err instanceof Error ? err.message : t('common.unknownError') }),
      )
      throw err
    }
  }

  const sortedKeys = [...keys].sort((a, b) => a.name.localeCompare(b.name))

  if (sortedKeys.length === 0) {
    return (
      <>
        <EmptyState
          title={t('providerKeys.emptyTitle')}
          description={t('providerKeys.emptyDesc')}
          action={
            <Button onClick={() => setCreateDialogOpen(true)}>
              {t('providerKeys.createKey')}
            </Button>
          }
        />

        <CreateKeyDialog
          open={createDialogOpen}
          onClose={() => setCreateDialogOpen(false)}
          onSubmit={handleCreateKey}
          profiles={PROFILES}
        />
      </>
    )
  }

  const handleDeleteKey = async (name: string) => {
    if (!activeZoneId) return
    setDeleting(true)
    try {
      await remove(activeZoneId, name)
      setDeleteConfirm({ open: false })
    } catch (err) {
      toastError(
        t('providerKeys.errors.deleteFailed', { error: err instanceof Error ? err.message : t('common.unknownError') }),
      )
    } finally {
      setDeleting(false)
    }
  }

  return (
    <>
      <div className="flex flex-col h-full">
        {/* Header */}
        <div className="h-12 border-b px-4 flex items-center justify-between shrink-0">
          <h3 className="text-sm font-medium">{t('providerKeys.title')}</h3>
          <Button size="sm" onClick={() => setCreateDialogOpen(true)}>
            {t('providerKeys.createKey')}
          </Button>
        </div>

        {/* List */}
        <div className="flex-1 overflow-y-auto">
          <div className="divide-y">
            {sortedKeys.map((key) => (
              <div key={key.id} className="p-4 hover:bg-muted/50 transition-colors">
                <div className="flex items-start justify-between gap-3 mb-2">
                  <div className="flex-1">
                    <div className="font-medium text-sm text-foreground">{key.name}</div>
                    <div className="text-xs text-muted-foreground mt-1">
                      {t('providerKeys.profileLabel')}: <Badge variant="default">{profileLabel(key.profileName)}</Badge>
                    </div>
                  </div>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => setDeleteConfirm({ open: true, name: key.name })}
                      className="text-xs text-destructive hover:text-destructive/80 h-auto p-0"
                    >
                      {t('common.delete')}
                    </Button>
                  </div>
                  <div className="space-y-1 text-xs text-muted-foreground">
                    <div>
                      {t('providerKeys.lastUsed')}: {formatRelativeTime(key.lastUsedAt)}
                    </div>
                    {key.baseUrl && (
                      <div>
                        {t('providerKeys.baseUrl')}: {key.baseUrl}
                      </div>
                    )}
                  </div>
              </div>
            ))}
          </div>
        </div>
      </div>

      {/* Create Dialog */}
      <CreateKeyDialog
        open={createDialogOpen}
        onClose={() => setCreateDialogOpen(false)}
        onSubmit={handleCreateKey}
        profiles={PROFILES}
      />

      {/* Delete Confirm Dialog */}
      <ConfirmDialog
        open={deleteConfirm.open}
        onClose={() => setDeleteConfirm({ open: false })}
        onConfirm={() => {
          if (deleteConfirm.name) {
            handleDeleteKey(deleteConfirm.name)
          }
        }}
        title={t('providerKeys.deleteConfirmTitle')}
        message={t('providerKeys.deleteConfirmMessage')}
        confirmLabel={t('common.delete')}
        variant="danger"
        loading={deleting}
      />
    </>
  )
}

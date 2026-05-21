import { useState } from 'react'
import { Modal, Input, Select, Button } from '@/components/ui'
import type { CreateCredentialInput } from '@/lib/types'
import { useTranslation } from 'react-i18next'

export type ProfileOption = {
  value: string
  label: string
  requiresBaseUrl: boolean
}

interface CreateKeyDialogProps {
  open: boolean
  onClose: () => void
  onSubmit: (input: CreateCredentialInput) => Promise<void>
  profiles: ProfileOption[]
}

export function CreateKeyDialog({ open, onClose, onSubmit, profiles }: CreateKeyDialogProps) {
  const { t } = useTranslation()
  const [name, setName] = useState('')
  const [profile, setProfile] = useState('')
  const [baseUrl, setBaseUrl] = useState('')
  const [key, setKey] = useState('')
  const [loading, setLoading] = useState(false)
  const [errors, setErrors] = useState<Record<string, string>>({})

  const selectedProfile = profiles.find((p) => p.value === profile)
  const requiresBaseUrl = selectedProfile?.requiresBaseUrl ?? false

  const validate = () => {
    const newErrors: Record<string, string> = {}
    if (!name.trim()) newErrors.name = t('providerKeys.dialog.errors.nameRequired')
    if (!profile) newErrors.profile = t('providerKeys.dialog.errors.profileRequired')
    if (requiresBaseUrl && !baseUrl.trim()) newErrors.baseUrl = t('providerKeys.dialog.errors.baseUrlRequired')
    if (!key.trim()) newErrors.key = t('providerKeys.dialog.errors.apiKeyRequired')
    setErrors(newErrors)
    return Object.keys(newErrors).length === 0
  }

  const handleSubmit = async () => {
    if (!validate()) return
    setLoading(true)
    try {
      const input: CreateCredentialInput = {
        name: name.trim(),
        profileName: profile,
        key: key.trim(),
        ...(requiresBaseUrl && baseUrl.trim() && { baseUrl: baseUrl.trim() }),
      }
      await onSubmit(input)
      // Reset form on success
      setName('')
      setProfile('')
      setBaseUrl('')
      setKey('')
      setErrors({})
      onClose()
    } catch {
      // Error handled by parent component (toast)
    } finally {
      setLoading(false)
    }
  }

  const handleClose = () => {
    if (!loading) {
      onClose()
      setName('')
      setProfile('')
      setBaseUrl('')
      setKey('')
      setErrors({})
    }
  }

  return (
    <Modal
      open={open}
      onClose={handleClose}
      title={t('providerKeys.dialog.title')}
      size="sm"
      footer={
        <>
          <Button variant="secondary" onClick={handleClose} disabled={loading}>
            {t('common.cancel')}
          </Button>
          <Button
            onClick={handleSubmit}
            disabled={loading}
            loading={loading}
          >
            {t('common.create')}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <Input
          label={t('providerKeys.dialog.keyName')}
          placeholder={t('providerKeys.dialog.keyNamePlaceholder')}
          value={name}
          onChange={(e) => {
            setName(e.target.value)
            if (errors.name) setErrors({ ...errors, name: '' })
          }}
          error={errors.name}
        />

        <Select
          label={t('providerKeys.dialog.provider')}
          value={profile}
          onChange={(e) => {
            setProfile(e.target.value)
            if (errors.profile) setErrors({ ...errors, profile: '' })
          }}
          options={profiles}
          error={errors.profile}
        />

        {requiresBaseUrl && (
          <Input
            label={t('providerKeys.dialog.baseUrl')}
            placeholder={t('providerKeys.dialog.baseUrlPlaceholder')}
            value={baseUrl}
            onChange={(e) => {
              setBaseUrl(e.target.value)
              if (errors.baseUrl) setErrors({ ...errors, baseUrl: '' })
            }}
            error={errors.baseUrl}
          />
        )}

        <Input
          label={t('providerKeys.dialog.apiKey')}
          type="password"
          placeholder={t('providerKeys.dialog.apiKeyPlaceholder')}
          value={key}
          onChange={(e) => {
            setKey(e.target.value)
            if (errors.key) setErrors({ ...errors, key: '' })
          }}
          error={errors.key}
        />
      </div>
    </Modal>
  )
}

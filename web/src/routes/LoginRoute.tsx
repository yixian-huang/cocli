import { useEffect } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useUserStore } from '@/stores/userStore'
import { LoginPage } from '@/components/LoginPage'

export function LoginRoute() {
  const { t } = useTranslation()
  const user = useUserStore((s) => s.user)
  const loading = useUserStore((s) => s.loading)
  const navigate = useNavigate()
  const location = useLocation()

  useEffect(() => {
    if (loading || !user) return
    const from = (location.state as { from?: string } | null)?.from
    const target = from && from !== '/login' ? from : '/'
    navigate(target, { replace: true })
  }, [user, loading, navigate, location.state])

  if (loading) {
    return (
      <div className="flex items-center justify-center h-screen">
        <div className="text-muted-foreground text-sm">{t('common.loading')}</div>
      </div>
    )
  }

  if (user) return null

  return <LoginPage />
}

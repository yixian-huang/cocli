import { useParams, useNavigate } from 'react-router-dom'
import { InviteSignup } from '@/components/InviteSignup'

export function InviteRoute() {
  const { code } = useParams<{ code: string }>()
  const navigate = useNavigate()

  if (!code) return null

  return (
    <InviteSignup
      code={code}
      onSuccess={() => {
        navigate('/')
        window.location.reload()
      }}
    />
  )
}

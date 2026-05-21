import { useState, useEffect } from 'react'
import { auth as authApi, setApiKey } from '@/api/client'
import { Button, Input } from '@/components/ui'
import { BRAND } from '@/brand'
import { BrandLogo } from '@/components/BrandLogo'

interface Props {
  code: string
  onSuccess: () => void
}

export function InviteSignup({ code, onSuccess }: Props) {
  const [username, setUsername] = useState('')
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [confirmPassword, setConfirmPassword] = useState('')
  const [error, setError] = useState('')
  const [checking, setChecking] = useState(true)
  const [valid, setValid] = useState(false)

  useEffect(() => {
    authApi.checkInvite(code).then((data) => {
      setValid(data.valid)
      setChecking(false)
    }).catch(() => {
      setValid(false)
      setChecking(false)
    })
  }, [code])

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setError('')

    if (password !== confirmPassword) {
      setError('Passwords do not match')
      return
    }
    if (password.length < 8) {
      setError('Password must be at least 8 characters')
      return
    }
    if (!email.trim().includes('@')) {
      setError('Please enter a valid email address')
      return
    }

    try {
      const data = await authApi.signup(code, username.trim(), email.trim(), password)
      setApiKey(data.apiKey)
      onSuccess()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Signup failed')
    }
  }

  if (checking) {
    return (
      <div className="flex items-center justify-center h-screen bg-background">
        <p className="text-muted-foreground">Checking invite...</p>
      </div>
    )
  }

  if (!valid) {
    return (
      <div className="flex items-center justify-center h-screen bg-background">
        <div className="text-center space-y-2 p-6">
          <h1 className="text-2xl font-bold">Invalid Invite</h1>
          <p className="text-sm text-muted-foreground">
            This invite link is expired, has reached its usage limit, or does not exist.
          </p>
        </div>
      </div>
    )
  }

  return (
    <div className="flex items-center justify-center h-screen bg-background">
      <form onSubmit={handleSubmit} className="w-full max-w-sm space-y-4 p-6">
        <div className="text-center space-y-2">
          <div className="flex justify-center">
            <BrandLogo iconClassName="h-10 w-10" textClassName="text-2xl" />
          </div>
          <h1 className="text-2xl font-bold">Join {BRAND.displayName}</h1>
          <p className="text-sm text-muted-foreground">Create your account</p>
        </div>

        <Input
          type="text"
          value={username}
          onChange={(e) => setUsername(e.target.value)}
          placeholder="Username"
          autoFocus
        />
        <Input
          type="email"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          placeholder="Email"
          autoComplete="email"
        />
        <Input
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          placeholder="Password"
        />
        <Input
          type="password"
          value={confirmPassword}
          onChange={(e) => setConfirmPassword(e.target.value)}
          placeholder="Confirm password"
        />

        {error && <p className="text-sm text-destructive">{error}</p>}

        <Button
          type="submit"
          variant="primary"
          className="w-full"
          disabled={!username.trim() || !email.trim() || !password || !confirmPassword}
        >
          Create Account
        </Button>
      </form>
    </div>
  )
}

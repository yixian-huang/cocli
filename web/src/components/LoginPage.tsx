import { useState } from 'react'
import { useUserStore } from '@/stores/userStore'
import { auth as authApi, ApiError } from '@/api/client'
import { Button, Input, LanguageSwitcher } from '@/components/ui'
import { BrandLogo } from '@/components/BrandLogo'
import { useTranslation } from 'react-i18next'

export function LoginPage() {
  const { t } = useTranslation()
  const [mode, setMode] = useState<'password' | 'apikey' | 'signup'>('password')
  const [username, setUsername] = useState('')
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [confirmPassword, setConfirmPassword] = useState('')
  const [inviteCode, setInviteCode] = useState('')
  const [key, setKey] = useState('')
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)
  const login = useUserStore((s) => s.login)
  const loginWithPassword = useUserStore((s) => s.loginWithPassword)
  const signup = useUserStore((s) => s.signup)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setError('')

    if (mode === 'signup') {
      if (!email.trim().includes('@')) {
        setError(t('auth.errors.invalidEmail'))
        return
      }
      if (password !== confirmPassword) {
        setError(t('auth.errors.passwordsDontMatch'))
        return
      }
      if (password.length < 8) {
        setError(t('auth.errors.passwordTooShort'))
        return
      }
    }

    setLoading(true)
    try {
      if (mode === 'password') {
        await loginWithPassword(username.trim(), password)
      } else if (mode === 'apikey') {
        await login(key.trim())
      } else {
        // Validate invite code first
        const check = await authApi.checkInvite(inviteCode.trim())
        if (!check.valid) {
          setError(t('auth.errors.invalidInviteCode'))
          setLoading(false)
          return
        }
        await signup(inviteCode.trim(), username.trim(), email.trim(), password)
      }
    } catch (err) {
      if (mode === 'signup') {
        const msg = err instanceof Error ? err.message : ''
        if (msg.includes('username already taken')) {
          setError(t('auth.errors.usernameTaken'))
        } else if (msg.includes('email already taken')) {
          setError(t('auth.errors.emailTaken'))
        } else if (msg.includes('invalid email')) {
          setError(t('auth.errors.invalidEmail'))
        } else {
          setError(t('auth.errors.signupFailed'))
        }
      } else if (mode === 'password') {
        setError(t('auth.errors.invalidUsernameEmailOrPassword'))
      } else if (err instanceof ApiError && err.status === 401) {
        setError(t('auth.errors.invalidApiKey'))
      } else if (import.meta.env.DEV) {
        const hint = err instanceof Error ? err.message : String(err)
        setError(
          `${t('auth.errors.apiUnreachable')}${hint ? ` (${hint})` : ''}. ${t('auth.errors.localDevUrl')}`,
        )
      } else {
        setError(t('auth.errors.invalidApiKey'))
      }
    } finally {
      setLoading(false)
    }
  }

  const switchMode = (newMode: 'password' | 'apikey' | 'signup') => {
    setMode(newMode)
    setError('')
  }

  const isSignupDisabled =
    !inviteCode.trim() || !username.trim() || !email.trim() || !password || !confirmPassword
  const isLoginDisabled = mode === 'password' ? !username.trim() || !password : !key.trim()

  return (
    <div className="flex items-center justify-center h-screen bg-background">
      <form onSubmit={handleSubmit} className="w-full max-w-sm space-y-4 p-8 bg-card rounded-xl shadow-whisper">
        <div className="text-center space-y-2 relative">
          <div className="absolute -top-1 right-0">
            <LanguageSwitcher compact />
          </div>
          <BrandLogo className="justify-center" iconClassName="h-10 w-10" textClassName="text-2xl" />
          <p className="text-sm text-muted-foreground">
            {mode === 'signup'
              ? t('auth.createAccount')
              : mode === 'password'
                ? t('auth.signInToContinue')
                : t('auth.enterApiKey')}
          </p>
        </div>

        {mode === 'signup' ? (
          <>
            <Input
              type="text"
              value={inviteCode}
              onChange={(e) => setInviteCode(e.target.value)}
              placeholder={t('auth.inviteCode')}
              autoFocus
            />
            <Input
              type="text"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              placeholder={t('auth.username')}
            />
            <Input
              type="email"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              placeholder={t('auth.email')}
              autoComplete="email"
            />
            <Input
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder={t('auth.password')}
            />
            <Input
              type="password"
              value={confirmPassword}
              onChange={(e) => setConfirmPassword(e.target.value)}
              placeholder={t('auth.confirmPassword')}
            />
          </>
        ) : mode === 'password' ? (
          <>
            <Input
              type="text"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              placeholder={t('auth.usernameOrEmail')}
              autoComplete="username"
              autoFocus
            />
            <Input
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder={t('auth.password')}
            />
          </>
        ) : (
          <Input
            type="password"
            value={key}
            onChange={(e) => setKey(e.target.value)}
            placeholder={t('auth.apiKey')}
            autoFocus
          />
        )}

        {error && <p className="text-sm text-destructive">{error}</p>}

        <Button
          type="submit"
          variant="primary"
          className="w-full"
          disabled={loading || (mode === 'signup' ? isSignupDisabled : isLoginDisabled)}
        >
          {loading
            ? t('auth.pleaseWait')
            : mode === 'signup'
              ? t('auth.createAccountButton')
              : mode === 'password'
                ? t('auth.signIn')
                : t('auth.connect')}
        </Button>

        {mode === 'signup' ? (
          <Button
            type="button"
            variant="ghost"
            className="w-full"
            onClick={() => switchMode('password')}
          >
            {t('auth.alreadyHaveAccount')}
          </Button>
        ) : (
          <div className="space-y-2">
            <Button
              type="button"
              variant="ghost"
              className="w-full"
              onClick={() => switchMode('signup')}
            >
              {t('auth.dontHaveAccount')}
            </Button>
            <Button
              type="button"
              variant="ghost"
              className="w-full text-xs"
              onClick={() => switchMode(mode === 'password' ? 'apikey' : 'password')}
            >
              {mode === 'password' ? t('auth.loginWithApiKeyInstead') : t('auth.loginWithPasswordInstead')}
            </Button>
          </div>
        )}
      </form>
    </div>
  )
}

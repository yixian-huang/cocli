import { useState, useRef, useEffect } from 'react'
import { useNavigate } from 'react-router-dom'
import { useUserStore } from '@/stores/userStore'
import { users as usersApi, auth as authApi } from '@/api/client'
import { toast, toastError } from '@/stores/toastStore'
import { cn } from '@/lib/utils'
import { User, Settings, Bell, Lock, Server } from 'lucide-react'
import { useZoneStore } from '@/stores/zoneStore'
import { storageKey } from '@/brand'
import { ThemeSection } from './ThemeSection'

export function UserProfile() {
  const navigate = useNavigate()
  const user = useUserStore((s) => s.user)
  const setUser = useUserStore((s) => s.setUser)
  const zoneSlug = useZoneStore((s) => s.activeZoneSlug)
  const [open, setOpen] = useState(false)
  const [displayName, setDisplayName] = useState(user?.displayName || '')
  const [saving, setSaving] = useState(false)
  const [notifyPref, setNotifyPref] = useState(() => localStorage.getItem(storageKey('notify')) || 'mentions')
  const [soundEnabled, setSoundEnabled] = useState(() => localStorage.getItem(storageKey('sound')) !== 'off')
  const [showPasswordForm, setShowPasswordForm] = useState(false)
  const [currentPassword, setCurrentPassword] = useState('')
  const [newPassword, setNewPassword] = useState('')
  const [confirmPassword, setConfirmPassword] = useState('')
  const [savingPassword, setSavingPassword] = useState(false)
  const panelRef = useRef<HTMLDivElement>(null)

  // Close on click outside
  useEffect(() => {
    if (!open) return
    const handler = (e: MouseEvent) => {
      if (panelRef.current && !panelRef.current.contains(e.target as Node)) {
        setOpen(false)
      }
    }
    document.addEventListener('mousedown', handler)
    return () => document.removeEventListener('mousedown', handler)
  }, [open])

  // Sync display name when user changes
  useEffect(() => {
    setDisplayName(user?.displayName || '')
  }, [user?.displayName])

  if (!user) return null

  const handleSave = async () => {
    setSaving(true)
    try {
      const updated = await usersApi.updateProfile(displayName.trim())
      setUser(updated)
      toast('Profile updated', 'success')
      setOpen(false)
    } catch (err) {
      toastError(err instanceof Error ? err.message : 'Failed to update profile')
    } finally {
      setSaving(false)
    }
  }

  const resetPasswordForm = () => {
    setCurrentPassword('')
    setNewPassword('')
    setConfirmPassword('')
    setShowPasswordForm(false)
  }

  const handlePasswordSave = async () => {
    if (newPassword.length < 8) {
      toastError('Password must be at least 8 characters')
      return
    }
    if (newPassword !== confirmPassword) {
      toastError('Passwords do not match')
      return
    }
    setSavingPassword(true)
    try {
      await authApi.changePassword(currentPassword, newPassword)
      setUser({ ...user, hasPassword: true })
      toast('Password updated', 'success')
      resetPasswordForm()
    } catch (err) {
      toastError(err instanceof Error ? err.message : 'Failed to update password')
    } finally {
      setSavingPassword(false)
    }
  }

  return (
    <div className="relative" ref={panelRef}>
      <button
        onClick={() => setOpen(!open)}
        className="flex items-center gap-2 px-3 py-2 w-full hover:bg-accent/50 transition-colors text-left"
      >
        <div className="h-7 w-7 rounded-full bg-blue-100 dark:bg-blue-900 flex items-center justify-center shrink-0">
          <User className="h-3.5 w-3.5 text-blue-700 dark:text-blue-300" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="text-xs font-medium truncate">{user.displayName || user.name}</div>
          <div className="text-[10px] text-muted-foreground truncate">@{user.name}</div>
        </div>
        <Settings className="h-3 w-3 text-muted-foreground shrink-0" />
      </button>

      {open && (
        <div className="absolute bottom-full left-0 right-0 mb-1 bg-popover border rounded-lg shadow-lg p-3 space-y-3 z-50">
          <h4 className="text-xs font-semibold">Profile Settings</h4>
          <div className="space-y-1">
            <button
              onClick={() => {
                setOpen(false)
                if (zoneSlug) navigate(`/z/${zoneSlug}/daemons`)
              }}
              disabled={!zoneSlug}
              className={cn(
                'w-full flex items-center gap-2 px-2 py-1 rounded text-xs transition-colors',
                zoneSlug ? 'hover:bg-accent text-muted-foreground hover:text-foreground' : 'opacity-50 cursor-not-allowed text-muted-foreground',
              )}
            >
              <Server className="h-3 w-3 text-muted-foreground" />
              <span>Daemon Manage</span>
            </button>
          </div>
          <div className="space-y-1.5">
            <label className="text-[10px] text-muted-foreground font-medium">Username</label>
            <div className="text-xs text-foreground px-2 py-1.5 rounded bg-muted">@{user.name}</div>
          </div>
          <div className="space-y-1.5">
            <label className="text-[10px] text-muted-foreground font-medium">Display Name</label>
            <input
              type="text"
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              placeholder="Enter display name"
              className="w-full rounded border bg-background px-2 py-1.5 text-xs focus:outline-none focus:ring-1 focus:ring-ring"
            />
          </div>
          <button
            onClick={handleSave}
            disabled={saving}
            className="w-full rounded bg-primary text-primary-foreground py-1.5 text-xs font-medium hover:bg-primary/90 disabled:opacity-50"
          >
            {saving ? 'Saving...' : 'Save'}
          </button>

          <div className="border-t border-border-default pt-3">
            <ThemeSection />
          </div>

          <div className="border-t pt-3 space-y-2">
            <div className="flex items-center gap-1.5">
              <Bell className="h-3 w-3 text-muted-foreground" />
              <h4 className="text-xs font-semibold">Notifications</h4>
            </div>
            <div className="space-y-1">
              {(['all', 'mentions', 'none'] as const).map((value) => {
                const labels = { all: 'All messages', mentions: 'Mentions only', none: 'Off' }
                return (
                  <button
                    key={value}
                    onClick={() => {
                      setNotifyPref(value)
                      localStorage.setItem(storageKey('notify'), value)
                    }}
                    className={cn(
                      'w-full text-left px-2 py-1 rounded text-xs transition-colors',
                      notifyPref === value ? 'bg-primary/10 text-primary font-medium' : 'hover:bg-accent text-muted-foreground',
                    )}
                  >
                    {labels[value]}
                  </button>
                )
              })}
            </div>
            <label className="flex items-center gap-2 px-2 py-1 text-xs cursor-pointer">
              <input
                type="checkbox"
                checked={soundEnabled}
                onChange={(e) => {
                  setSoundEnabled(e.target.checked)
                  localStorage.setItem(storageKey('sound'), e.target.checked ? 'on' : 'off')
                }}
                className="rounded border-muted-foreground"
              />
              <span className="text-muted-foreground">Notification sound</span>
            </label>
          </div>

          <div className="border-t pt-3 space-y-2">
            <div className="flex items-center gap-1.5">
              <Lock className="h-3 w-3 text-muted-foreground" />
              <h4 className="text-xs font-semibold">Password</h4>
            </div>
            {!showPasswordForm ? (
              <button
                onClick={() => setShowPasswordForm(true)}
                className="w-full text-left px-2 py-1 rounded text-xs hover:bg-accent text-muted-foreground transition-colors"
              >
                {user.hasPassword ? 'Change password' : 'Set password'}
              </button>
            ) : (
              <div className="space-y-1.5">
                {user.hasPassword && (
                  <input
                    type="password"
                    value={currentPassword}
                    onChange={(e) => setCurrentPassword(e.target.value)}
                    placeholder="Current password"
                    className="w-full rounded border bg-background px-2 py-1.5 text-xs focus:outline-none focus:ring-1 focus:ring-ring"
                  />
                )}
                <input
                  type="password"
                  value={newPassword}
                  onChange={(e) => setNewPassword(e.target.value)}
                  placeholder="New password"
                  className="w-full rounded border bg-background px-2 py-1.5 text-xs focus:outline-none focus:ring-1 focus:ring-ring"
                />
                <input
                  type="password"
                  value={confirmPassword}
                  onChange={(e) => setConfirmPassword(e.target.value)}
                  placeholder="Confirm new password"
                  className="w-full rounded border bg-background px-2 py-1.5 text-xs focus:outline-none focus:ring-1 focus:ring-ring"
                />
                <div className="flex gap-1.5">
                  <button
                    onClick={handlePasswordSave}
                    disabled={savingPassword || !newPassword || !confirmPassword || (user.hasPassword && !currentPassword)}
                    className="flex-1 rounded bg-primary text-primary-foreground py-1.5 text-xs font-medium hover:bg-primary/90 disabled:opacity-50"
                  >
                    {savingPassword ? 'Saving...' : 'Save'}
                  </button>
                  <button
                    onClick={resetPasswordForm}
                    className="flex-1 rounded border py-1.5 text-xs font-medium hover:bg-accent transition-colors"
                  >
                    Cancel
                  </button>
                </div>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  )
}

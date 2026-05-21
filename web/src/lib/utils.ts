import { clsx, type ClassValue } from 'clsx'
import { twMerge } from 'tailwind-merge'

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

const AVATAR_COLORS = [
  ['bg-blue-500', 'text-white'],
  ['bg-emerald-500', 'text-white'],
  ['bg-amber-500', 'text-white'],
  ['bg-rose-500', 'text-white'],
  ['bg-cyan-500', 'text-white'],
  ['bg-purple-500', 'text-white'],
  ['bg-orange-500', 'text-white'],
  ['bg-teal-500', 'text-white'],
  ['bg-pink-500', 'text-white'],
  ['bg-indigo-500', 'text-white'],
] as const

export function avatarColor(name: string): [string, string] {
  let hash = 0
  for (let i = 0; i < name.length; i++) {
    hash = name.charCodeAt(i) + ((hash << 5) - hash)
  }
  return AVATAR_COLORS[Math.abs(hash) % AVATAR_COLORS.length] as [string, string]
}

export function avatarInitial(name: string): string {
  return (name[0] || '?').toUpperCase()
}

export function relativeTime(dateStr: string): string {
  const now = Date.now()
  const then = new Date(dateStr).getTime()
  const diff = now - then
  const seconds = Math.floor(diff / 1000)
  if (seconds < 60) return 'just now'
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes}m ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}h ago`
  const days = Math.floor(hours / 24)
  if (days < 7) return `${days}d ago`
  return new Date(dateStr).toLocaleDateString()
}

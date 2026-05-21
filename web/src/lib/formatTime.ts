/** Short locale datetime for daemon list/detail (e.g. May 20, 17:15). */
export function formatShortDateTime(dateStr: string | undefined | null): string {
  if (!dateStr) return '—'
  const d = new Date(dateStr)
  if (Number.isNaN(d.getTime())) return '—'
  return d.toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
}

/** Relative time for last-seen; "Now" when daemon is currently connected. */
export function formatLastConnection(
  lastSeen: string | undefined | null,
  connected: boolean,
): string {
  if (connected) return 'Now'
  if (!lastSeen) return 'Never'
  const date = new Date(lastSeen)
  if (Number.isNaN(date.getTime())) return 'Never'
  const seconds = Math.floor((Date.now() - date.getTime()) / 1000)
  if (seconds < 60) return 'Just now'
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`
  if (seconds < 604800) return `${Math.floor(seconds / 86400)}d ago`
  return formatShortDateTime(lastSeen)
}

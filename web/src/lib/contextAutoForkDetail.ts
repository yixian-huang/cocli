export type ContextAutoForkMode = 'native' | 'restart fallback'

const CONTEXT_AUTO_FORK_DETAIL_RE = /^(context_auto_fork_[a-z_]+)\s+\((native|restart fallback)\)$/i

export interface ParsedContextAutoForkDetail {
  text: string
  mode: ContextAutoForkMode | null
}

export function parseContextAutoForkDetail(detail?: string | null): ParsedContextAutoForkDetail {
  if (!detail) return { text: '', mode: null }
  const normalized = detail.trim()
  const match = normalized.match(CONTEXT_AUTO_FORK_DETAIL_RE)
  if (!match) {
    return { text: normalized, mode: null }
  }
  return {
    text: match[1],
    mode: match[2].toLowerCase() as ContextAutoForkMode,
  }
}

export function contextAutoForkModeVariant(mode: ContextAutoForkMode): 'success' | 'warning' {
  return mode === 'native' ? 'success' : 'warning'
}

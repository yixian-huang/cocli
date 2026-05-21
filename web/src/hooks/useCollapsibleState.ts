import { usePrefsStore, setCollapsed as setPref } from '@/stores/prefsStore'

export function useCollapsibleState(
  id: string,
  defaultCollapsed = false,
): [boolean, (next: boolean) => void] {
  const stored = usePrefsStore((s) => s.prefs.ui?.collapsed?.[id])
  const collapsed = stored ?? defaultCollapsed
  return [collapsed, (next) => setPref(id, next)]
}

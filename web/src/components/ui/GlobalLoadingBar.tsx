import { useSyncExternalStore } from 'react'
import { getInflight, subscribeInflight } from '@/api/client'

/**
 * Slim top-of-viewport progress bar driven by the api client's in-flight
 * counter. Renders nothing when there is no active request, so it stays
 * out of the user's way during steady state.
 */
export function GlobalLoadingBar() {
  const count = useSyncExternalStore(subscribeInflight, getInflight, getInflight)
  if (count === 0) return null
  return (
    <div
      aria-hidden
      className="pointer-events-none fixed inset-x-0 top-0 z-100 h-0.5 overflow-hidden"
    >
      <div className="h-full w-1/3 animate-[loading-bar_1.2s_ease-in-out_infinite] bg-primary/80" />
    </div>
  )
}

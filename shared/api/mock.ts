// shared/api/mock.ts
//
// Tiny stand-in for the future cocli-api crate. Powers `VITE_USE_MOCK=true`
// dev runs where there's no Rust backend yet. Returns empty/undefined for
// most paths; hardcodes a single `#general` channel + version/health so the
// router can navigate after the first-run wizard.

import type { Channel } from '@shared/types'

const channels: Channel[] = [
  {
    id: 'general',
    name: 'general',
    type: 'channel',
    description: 'Welcome to cocli local',
    createdAt: new Date().toISOString(),
  },
]

export async function mockHandler<T>(path: string, options: RequestInit): Promise<T> {
  const method = (options.method ?? 'GET').toUpperCase()
  const route = path.split('?')[0]

  if (method === 'GET' && route === '/api/channels') {
    return channels as unknown as T
  }
  if (method === 'GET' && route === '/api/version') {
    return { version: '0.0.0-mock', commit: 'mock' } as unknown as T
  }
  if (method === 'GET' && route === '/api/health') {
    return undefined as T
  }
  if (method === 'GET' && route === '/api/threads') {
    return { threads: [] } as unknown as T
  }

  if (method === 'GET') {
    return [] as unknown as T
  }
  return undefined as T
}

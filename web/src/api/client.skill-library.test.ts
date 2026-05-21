import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { zoneSkillLibrary, setApiKey } from './client'

const fetchMock = vi.fn()

beforeEach(() => {
  vi.stubGlobal('fetch', fetchMock)
  setApiKey('test-key')
  fetchMock.mockReset()
  fetchMock.mockResolvedValue({
    ok: true,
    status: 200,
    headers: new Headers({ 'content-type': 'application/json', 'x-request-id': 'rid-1' }),
    json: async () => ({ entries: [] }),
    text: async () => '{}',
  } as Response)
})
afterEach(() => vi.unstubAllGlobals())

describe('zoneSkillLibrary', () => {
  it('list() calls GET /api/zones/:zoneId/skills/library', async () => {
    await zoneSkillLibrary.list('z1')
    const [url, init] = fetchMock.mock.calls[0]
    expect(url).toBe('/api/zones/z1/skills/library')
    expect(init?.method ?? 'GET').toBe('GET')
    expect((init?.headers as Record<string, string>)['X-API-Key']).toBe('test-key')
  })

  it('import() POSTs URL/subPath/name as JSON body', async () => {
    await zoneSkillLibrary.import('z1', { url: 'https://github.com/x/y', subPath: 'skills/a', name: 'a' })
    const [url, init] = fetchMock.mock.calls[0]
    expect(url).toBe('/api/zones/z1/skills/library')
    expect(init?.method).toBe('POST')
    expect(JSON.parse(init?.body as string)).toEqual({ url: 'https://github.com/x/y', subPath: 'skills/a', name: 'a' })
  })

  it('import() forwards AbortSignal for cancel support', async () => {
    const ac = new AbortController()
    await zoneSkillLibrary.import('z1', { url: 'https://x' }, { signal: ac.signal })
    const [, init] = fetchMock.mock.calls[0]
    expect(init?.signal).toBe(ac.signal)
  })

  it('reinstall() POSTs to /:id/reinstall', async () => {
    await zoneSkillLibrary.reinstall('z1', 'lib-1')
    expect(fetchMock.mock.calls[0][0]).toBe('/api/zones/z1/skills/library/lib-1/reinstall')
    expect(fetchMock.mock.calls[0][1]?.method).toBe('POST')
  })

  it('remove() DELETEs the entry', async () => {
    await zoneSkillLibrary.remove('z1', 'lib-1')
    expect(fetchMock.mock.calls[0][1]?.method).toBe('DELETE')
  })

  it('getFile() URL-encodes the relPath wildcard', async () => {
    await zoneSkillLibrary.getFile('z1', 'lib-1', 'scripts/run.sh')
    expect(fetchMock.mock.calls[0][0]).toBe('/api/zones/z1/skills/library/lib-1/files/scripts%2Frun.sh')
  })
})

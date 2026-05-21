import { getApiKey } from '@/api/client'

export interface WikiPageSummary {
  path: string
  title: string
  tags: string[]
  updatedAt: string
  updatedBy?: string
}

export interface WikiPage extends WikiPageSummary {
  content: string
}

export interface WikiRevision {
  version: number
  content: string
  createdAt: string
  createdBy?: string
}

interface ListPagesParams {
  q?: string
  tag?: string
}

interface ListPagesResponse {
  pages: WikiPageSummary[]
}

interface ListRevisionsResponse {
  revisions: WikiRevision[]
}

export interface WikiBacklink {
  path: string
  title: string
  updatedAt: string
  version: number
}

interface ListBacklinksResponse {
  backlinks: WikiBacklink[]
}

async function request<T>(path: string, options: RequestInit = {}): Promise<T> {
  const res = await fetch(path, {
    ...options,
    headers: {
      'Content-Type': 'application/json',
      'X-Cocli-Token': getApiKey(),
      ...options.headers,
    },
  })
  if (!res.ok) {
    const body = await res.text()
    throw new Error(`${res.status}: ${body}`)
  }
  return res.json()
}

export function listPages(params: ListPagesParams = {}) {
  const qs = new URLSearchParams()
  if (params.q) qs.set('q', params.q)
  if (params.tag) qs.set('tag', params.tag)
  const query = qs.toString()
  return request<ListPagesResponse>(`/api/wiki/pages${query ? `?${query}` : ''}`)
}

export function getPage(path: string) {
  return request<WikiPage>(`/api/wiki/pages/${encodeURIComponent(path)}`)
}

export function listRevisions(path: string) {
  return request<ListRevisionsResponse>(`/api/wiki/pages/${encodeURIComponent(path)}/revisions`)
}

// listBacklinks returns pages whose markdown body links to `path` via
// a [[wikilink]] token. Used by the admin browser to render a
// "Referenced by" sidebar section on the page detail view.
export function listBacklinks(path: string) {
  return request<ListBacklinksResponse>(`/api/wiki/pages/${encodeURIComponent(path)}/backlinks`)
}

export async function revertPage(path: string, version: number) {
  const payload = await request<{ page: WikiPage } | WikiPage>(
    `/api/wiki/pages/${encodeURIComponent(path)}/revert`,
    {
      method: 'POST',
      body: JSON.stringify({ version }),
    },
  )
  if ('page' in payload) return payload.page
  return payload
}

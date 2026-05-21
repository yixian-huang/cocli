import { useCallback, useEffect, useMemo, useState } from 'react'
import { RefreshCw, Search, Tag } from 'lucide-react'
import { Button, Input, Textarea } from '@/components/ui'
import { ConfirmDialog } from '@/components/ui/ConfirmDialog'
import { MarkdownRenderer } from '@/components/chat/MarkdownRenderer'
import { useUserStore } from '@/stores/userStore'
import {
  getPage,
  listBacklinks,
  listPages,
  listRevisions,
  revertPage,
  type WikiBacklink,
  type WikiPage,
  type WikiPageSummary,
  type WikiRevision,
} from '@/lib/api/wiki'

function formatUpdatedAt(value: string) {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return date.toLocaleString()
}

function isNotFoundError(err: unknown) {
  return err instanceof Error && err.message.startsWith('404:')
}

export function WikiBrowser() {
  const isAdmin = useUserStore((s) => s.user?.role === 'admin')
  const [pages, setPages] = useState<WikiPageSummary[]>([])
  const [selectedPath, setSelectedPath] = useState<string>('')
  const [page, setPage] = useState<WikiPage | null>(null)
  const [revisions, setRevisions] = useState<WikiRevision[]>([])
  const [backlinks, setBacklinks] = useState<WikiBacklink[]>([])
  const [loadingBacklinks, setLoadingBacklinks] = useState(false)
  const [selectedVersion, setSelectedVersion] = useState('current')
  const [query, setQuery] = useState('')
  const [tag, setTag] = useState('')
  const [draft, setDraft] = useState('')
  const [editing, setEditing] = useState(false)
  const [revertTargetVersion, setRevertTargetVersion] = useState<number | null>(null)
  const [reverting, setReverting] = useState(false)
  const [loadingList, setLoadingList] = useState(false)
  const [loadingPage, setLoadingPage] = useState(false)
  const [loadingRevisions, setLoadingRevisions] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const sortedPages = useMemo(
    () =>
      [...pages].sort((a, b) => {
        const at = new Date(a.updatedAt).getTime()
        const bt = new Date(b.updatedAt).getTime()
        return bt - at
      }),
    [pages],
  )

  const loadPages = useCallback(async () => {
    setLoadingList(true)
    setError(null)
    try {
      const { pages: nextPages } = await listPages({
        q: query.trim() || undefined,
        tag: tag.trim() || undefined,
      })
      setPages(nextPages)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load wiki pages')
      setPages([])
    } finally {
      setLoadingList(false)
    }
  }, [query, tag])

  useEffect(() => {
    void loadPages()
  }, [loadPages])

  useEffect(() => {
    if (sortedPages.length === 0) {
      setSelectedPath('')
      setPage(null)
      return
    }
    setSelectedPath((prev) => {
      if (prev && sortedPages.some((entry) => entry.path === prev)) return prev
      return sortedPages[0].path
    })
  }, [sortedPages])

  useEffect(() => {
    if (!selectedPath) {
      setPage(null)
      setRevisions([])
      setSelectedVersion('current')
      return
    }
    let cancelled = false
    const run = async () => {
      setLoadingPage(true)
      setError(null)
      try {
        const nextPage = await getPage(selectedPath)
        if (!cancelled) setPage(nextPage)
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : 'Failed to load wiki page')
          setPage(null)
        }
      } finally {
        if (!cancelled) setLoadingPage(false)
      }
    }
    void run()
    return () => {
      cancelled = true
    }
  }, [selectedPath])

  useEffect(() => {
    if (!selectedPath) {
      setRevisions([])
      setSelectedVersion('current')
      return
    }
    let cancelled = false
    const run = async () => {
      setLoadingRevisions(true)
      try {
        const { revisions: nextRevisions } = await listRevisions(selectedPath)
        if (!cancelled) setRevisions(nextRevisions)
      } catch (err) {
        if (!cancelled) {
          if (!isNotFoundError(err)) {
            setError(err instanceof Error ? err.message : 'Failed to load revisions')
          }
          setRevisions([])
        }
      } finally {
        if (!cancelled) setLoadingRevisions(false)
      }
    }
    void run()
    return () => {
      cancelled = true
    }
  }, [selectedPath])

  useEffect(() => {
    setSelectedVersion('current')
  }, [selectedPath])

  // Fetch backlinks for the currently selected page. Kept in its own effect
  // (rather than piggy-backing the main page fetch) so a backlinks error
  // doesn't clobber the already-rendered body — backlinks are a sidebar
  // hint, not the primary content.
  useEffect(() => {
    if (!selectedPath) {
      setBacklinks([])
      return
    }
    let cancelled = false
    const run = async () => {
      setLoadingBacklinks(true)
      try {
        const { backlinks: next } = await listBacklinks(selectedPath)
        if (!cancelled) setBacklinks(next)
      } catch {
        if (!cancelled) setBacklinks([])
      } finally {
        if (!cancelled) setLoadingBacklinks(false)
      }
    }
    void run()
    return () => {
      cancelled = true
    }
  }, [selectedPath])

  useEffect(() => {
    if (!page) {
      setDraft('')
      return
    }
    setDraft(page.content)
  }, [page])

  const selectedRevision = useMemo(
    () => revisions.find((item) => String(item.version) === selectedVersion) ?? null,
    [revisions, selectedVersion],
  )
  const viewingHistoricalRevision = selectedVersion !== 'current'
  const displayedContent = selectedRevision?.content ?? page?.content ?? ''

  return (
    <div className="flex-1 min-h-0 flex flex-col">
      <div className="h-12 border-b px-4 flex items-center justify-between">
        <div className="text-sm font-semibold">Wiki Admin Browser</div>
        <Button variant="ghost" size="sm" className="gap-1" onClick={() => void loadPages()}>
          <RefreshCw className="h-3.5 w-3.5" />
          Refresh
        </Button>
      </div>

      {error && (
        <div className="px-4 py-2 border-b text-xs text-error">{error}</div>
      )}

      <div className="flex-1 min-h-0 grid grid-cols-1 lg:grid-cols-[20rem_1fr]">
        <aside className="border-r border-border min-h-0 flex flex-col">
          <div className="p-3 border-b border-border/60 space-y-2">
            <label className="block">
              <div className="mb-1 text-[11px] text-muted-foreground flex items-center gap-1">
                <Search className="h-3.5 w-3.5" />
                Search
              </div>
              <Input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="title/path keyword"
              />
            </label>
            <label className="block">
              <div className="mb-1 text-[11px] text-muted-foreground flex items-center gap-1">
                <Tag className="h-3.5 w-3.5" />
                Tag filter (placeholder)
              </div>
              <Input
                value={tag}
                onChange={(e) => setTag(e.target.value)}
                placeholder="e.g. protocol"
              />
            </label>
          </div>

          <div className="flex-1 min-h-0 overflow-y-auto">
            {loadingList ? (
              <div className="p-3 text-xs text-muted-foreground">Loading pages...</div>
            ) : sortedPages.length === 0 ? (
              <div className="p-3 text-xs text-muted-foreground">No wiki pages.</div>
            ) : (
              <div className="divide-y divide-border/60">
                {sortedPages.map((entry) => {
                  const active = entry.path === selectedPath
                  return (
                    <button
                      key={entry.path}
                      type="button"
                      onClick={() => {
                        setEditing(false)
                        setSelectedPath(entry.path)
                      }}
                      className={`w-full px-3 py-2 text-left hover:bg-accent/40 ${active ? 'bg-primary/10' : ''}`}
                    >
                      <div className="text-sm font-medium truncate">{entry.title || entry.path}</div>
                      <div className="text-[11px] text-muted-foreground truncate">{entry.path}</div>
                      <div className="mt-1 text-[10px] text-muted-foreground">
                        Updated {formatUpdatedAt(entry.updatedAt)}
                      </div>
                    </button>
                  )
                })}
              </div>
            )}
          </div>
        </aside>

        <section className="min-h-0 flex flex-col">
          {!selectedPath ? (
            <div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
              Select a wiki page.
            </div>
          ) : loadingPage ? (
            <div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
              Loading page...
            </div>
          ) : !page ? (
            <div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
              Page unavailable.
            </div>
          ) : (
            <>
              <div className="h-12 border-b px-4 flex items-center gap-2">
                <div className="min-w-0">
                  <div className="text-sm font-semibold truncate">{page.title || page.path}</div>
                  <div className="text-[11px] text-muted-foreground truncate">{page.path}</div>
                </div>
                <div className="ml-auto flex items-center gap-2">
                  <div className="text-[11px] text-muted-foreground">
                    Revision
                  </div>
                  <select
                    className="h-8 min-w-[9rem] rounded border border-border bg-background px-2 text-xs"
                    value={selectedVersion}
                    onChange={(e) => setSelectedVersion(e.target.value)}
                    disabled={loadingRevisions}
                    aria-label="Revision"
                  >
                    <option value="current">Current</option>
                    {revisions.map((revision) => (
                      <option key={revision.version} value={String(revision.version)}>
                        v{revision.version} · {formatUpdatedAt(revision.createdAt)}
                      </option>
                    ))}
                  </select>
                  {isAdmin && viewingHistoricalRevision && selectedRevision && (
                    <Button
                      variant="danger"
                      size="sm"
                      onClick={() => setRevertTargetVersion(selectedRevision.version)}
                    >
                      Revert
                    </Button>
                  )}
                  <div className="text-[11px] text-muted-foreground hidden sm:block">
                    {formatUpdatedAt(page.updatedAt)}
                  </div>
                  <Button
                    variant={editing ? 'secondary' : 'primary'}
                    size="sm"
                    onClick={() => setEditing((prev) => !prev)}
                    disabled={viewingHistoricalRevision}
                    title={viewingHistoricalRevision ? 'Switch to Current revision to edit' : undefined}
                  >
                    {editing ? 'Preview' : 'Edit'}
                  </Button>
                </div>
              </div>
              <div className="flex-1 min-h-0 overflow-y-auto p-4">
                {editing ? (
                  <div className="space-y-2">
                    <div className="text-xs text-muted-foreground">
                      Rich editor disabled in R1 — using plain textarea.
                    </div>
                    <Textarea
                      value={draft}
                      onChange={(e) => setDraft(e.target.value)}
                      className="min-h-[60vh] font-mono text-xs"
                    />
                  </div>
                ) : (
                  <article className="prose prose-sm dark:prose-invert max-w-none">
                    <MarkdownRenderer>{displayedContent}</MarkdownRenderer>
                  </article>
                )}
                {!editing && !viewingHistoricalRevision && (
                  <section
                    aria-label="Backlinks"
                    className="mt-6 border-t border-border/60 pt-3 text-xs"
                  >
                    <div className="mb-1 font-semibold text-muted-foreground">Referenced by</div>
                    {loadingBacklinks ? (
                      <div className="text-muted-foreground">Loading backlinks...</div>
                    ) : backlinks.length === 0 ? (
                      <div className="text-muted-foreground">No pages link here.</div>
                    ) : (
                      <ul className="space-y-1">
                        {backlinks.map((entry) => (
                          <li key={entry.path}>
                            <button
                              type="button"
                              onClick={() => {
                                setEditing(false)
                                setSelectedPath(entry.path)
                              }}
                              className="text-left hover:underline"
                            >
                              [[{entry.title || entry.path}]]
                            </button>
                            <span className="ml-2 text-muted-foreground">{entry.path}</span>
                          </li>
                        ))}
                      </ul>
                    )}
                  </section>
                )}
              </div>
            </>
          )}
        </section>
      </div>
      <ConfirmDialog
        open={revertTargetVersion != null}
        onClose={() => {
          if (reverting) return
          setRevertTargetVersion(null)
        }}
        onConfirm={async () => {
          if (!selectedPath || revertTargetVersion == null) return
          setReverting(true)
          setError(null)
          try {
            const nextPage = await revertPage(selectedPath, revertTargetVersion)
            setPage(nextPage)
            const { revisions: nextRevisions } = await listRevisions(selectedPath)
            setRevisions(nextRevisions)
            void loadPages()
            setSelectedVersion('current')
            setEditing(false)
          } catch (err) {
            setError(err instanceof Error ? err.message : 'Failed to revert revision')
          } finally {
            setReverting(false)
            setRevertTargetVersion(null)
          }
        }}
        loading={reverting}
        title="Revert wiki page?"
        message={revertTargetVersion == null ? '' : `This will create a new head revision from v${revertTargetVersion}.`}
        confirmLabel="Revert"
        variant="danger"
      />
    </div>
  )
}

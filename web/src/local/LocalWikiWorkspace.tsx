import {
  BookOpen,
  Clock3,
  Eye,
  FilePlus2,
  Link2,
  Pencil,
  RefreshCw,
  RotateCcw,
  Save,
} from 'lucide-react'
import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type FormEvent,
} from 'react'
import { MarkdownRenderer } from '../components/chat/MarkdownRenderer'
import {
  localApi,
  type WikiBacklink,
  type WikiPage,
  type WikiPageSummary,
  type WikiRevision,
} from './api'
import type { LocalCopyKey } from './localization'

interface LocalWikiWorkspaceProps {
  t: (key: LocalCopyKey, values?: Record<string, string | number>) => string
}

function formatDate(value: string): string {
  const date = new Date(value)
  return Number.isNaN(date.getTime())
    ? value
    : date.toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' })
}

function parseTags(value: string): string[] {
  return [...new Set(value.split(',').map((tag) => tag.trim()).filter(Boolean))]
}

function isUserWikiPage(page: WikiPageSummary): boolean {
  return !(
    /^agents\/[^/]+\/memory\//.test(page.path)
    || /^channels\/[^/]+\/notes\//.test(page.path)
  )
}

export function LocalWikiWorkspace({ t }: LocalWikiWorkspaceProps) {
  const [pages, setPages] = useState<WikiPageSummary[]>([])
  const [selectedPath, setSelectedPath] = useState('')
  const [page, setPage] = useState<WikiPage | null>(null)
  const [revisions, setRevisions] = useState<WikiRevision[]>([])
  const [backlinks, setBacklinks] = useState<WikiBacklink[]>([])
  const [query, setQuery] = useState('')
  const [tag, setTag] = useState('')
  const [editing, setEditing] = useState(false)
  const [selectedVersion, setSelectedVersion] = useState<'current' | number>('current')
  const [newPageOpen, setNewPageOpen] = useState(false)
  const [newPath, setNewPath] = useState('')
  const [newTitle, setNewTitle] = useState('')
  const [newContent, setNewContent] = useState('')
  const [newTags, setNewTags] = useState('')
  const [newReason, setNewReason] = useState('')
  const [draftTitle, setDraftTitle] = useState('')
  const [draftContent, setDraftContent] = useState('')
  const [draftTags, setDraftTags] = useState('')
  const [draftReason, setDraftReason] = useState('')
  const [loading, setLoading] = useState(false)
  const [action, setAction] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  const allTags = useMemo(
    () => [...new Set(pages.flatMap((candidate) => candidate.tags))].sort(),
    [pages],
  )
  const historicalRevision = useMemo(
    () => selectedVersion === 'current'
      ? null
      : revisions.find((revision) => revision.version === selectedVersion) ?? null,
    [revisions, selectedVersion],
  )
  const displayedTitle = historicalRevision?.title ?? page?.title ?? ''
  const displayedContent = historicalRevision?.content ?? page?.content ?? ''
  const displayedTags = historicalRevision?.tags ?? page?.tags ?? []

  const loadPages = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const response = await localApi.listWikiPages(query.trim() || undefined, tag || undefined)
      const nextPages = response.pages.filter(isUserWikiPage)
      setPages(nextPages)
      setSelectedPath((current) => (
        nextPages.some((candidate) => candidate.path === current) ? current : ''
      ))
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : t('wikiLoadError'))
    } finally {
      setLoading(false)
    }
  }, [query, tag, t])

  useEffect(() => {
    const timer = window.setTimeout(() => void loadPages(), 180)
    return () => window.clearTimeout(timer)
  }, [loadPages])

  useEffect(() => {
    if (!selectedPath) {
      setPage(null)
      setRevisions([])
      setBacklinks([])
      return
    }
    let cancelled = false
    setLoading(true)
    setError(null)
    Promise.all([
      localApi.getWikiPage(selectedPath),
      localApi.listWikiRevisions(selectedPath),
      localApi.listWikiBacklinks(selectedPath),
    ])
      .then(([nextPage, revisionResponse, backlinkResponse]) => {
        if (cancelled) return
        setPage(nextPage)
        setRevisions(revisionResponse.revisions)
        setBacklinks(backlinkResponse.backlinks)
        setDraftTitle(nextPage.title)
        setDraftContent(nextPage.content)
        setDraftTags(nextPage.tags.join(', '))
        setDraftReason('')
        setSelectedVersion('current')
        setEditing(false)
      })
      .catch((nextError: unknown) => {
        if (!cancelled) {
          setError(nextError instanceof Error ? nextError.message : t('wikiLoadError'))
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [selectedPath, t])

  const runAction = useCallback(async (key: string, task: () => Promise<void>) => {
    setAction(key)
    setError(null)
    try {
      await task()
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : t('wikiActionError'))
    } finally {
      setAction(null)
    }
  }, [t])

  function createPage(event: FormEvent) {
    event.preventDefault()
    const path = newPath.trim().replace(/^\/+|\/+$/g, '')
    if (!path || !newTitle.trim()) return
    void runAction('create', async () => {
      const created = await localApi.upsertWikiPage(path, {
        title: newTitle.trim(),
        content: newContent,
        tags: parseTags(newTags),
        updatedBy: 'local-user',
        reason: newReason.trim() || undefined,
      })
      setNewPath('')
      setNewTitle('')
      setNewContent('')
      setNewTags('')
      setNewReason('')
      setNewPageOpen(false)
      await loadPages()
      setSelectedPath(created.path)
    })
  }

  function savePage(event: FormEvent) {
    event.preventDefault()
    if (!page) return
    void runAction('save', async () => {
      const updated = await localApi.upsertWikiPage(page.path, {
        title: draftTitle.trim(),
        content: draftContent,
        tags: parseTags(draftTags),
        updatedBy: 'local-user',
        reason: draftReason.trim() || undefined,
        ifVersion: page.version,
      })
      setPage(updated)
      setDraftReason('')
      setEditing(false)
      const [revisionResponse, backlinkResponse] = await Promise.all([
        localApi.listWikiRevisions(updated.path),
        localApi.listWikiBacklinks(updated.path),
      ])
      setRevisions(revisionResponse.revisions)
      setBacklinks(backlinkResponse.backlinks)
      await loadPages()
    })
  }

  function revertRevision(version: number) {
    if (!page || !window.confirm(t('wikiRevertConfirm', { version }))) return
    void runAction('revert', async () => {
      const response = await localApi.revertWikiPage(page.path, version)
      setPage(response.page)
      setDraftTitle(response.page.title)
      setDraftContent(response.page.content)
      setDraftTags(response.page.tags.join(', '))
      setSelectedVersion('current')
      const revisionResponse = await localApi.listWikiRevisions(response.page.path)
      setRevisions(revisionResponse.revisions)
      await loadPages()
    })
  }

  return (
    <div className="wiki-workspace">
      <aside className="knowledge-sidebar">
        <div className="knowledge-controls">
          <div className="knowledge-filter-row">
            <input
              aria-label={t('wikiSearch')}
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder={t('wikiSearchPlaceholder')}
            />
            <select
              aria-label={t('wikiTag')}
              value={tag}
              onChange={(event) => setTag(event.target.value)}
            >
              <option value="">{t('wikiAllTags')}</option>
              {allTags.map((candidate) => (
                <option key={candidate} value={candidate}>{candidate}</option>
              ))}
            </select>
          </div>
          <div className="knowledge-sidebar-actions">
            <button type="button" onClick={() => setNewPageOpen((current) => !current)}>
              <FilePlus2 size={14} aria-hidden="true" />
              {t('wikiNewPage')}
            </button>
            <button type="button" aria-label={t('refresh')} onClick={() => void loadPages()}>
              <RefreshCw size={14} aria-hidden="true" />
            </button>
          </div>
        </div>

        {newPageOpen && (
          <form className="wiki-create-form" onSubmit={createPage}>
            <label>
              <span>{t('wikiPath')}</span>
              <input
                value={newPath}
                onChange={(event) => setNewPath(event.target.value)}
                placeholder={t('wikiPathPlaceholder')}
                required
              />
            </label>
            <label>
              <span>{t('wikiPageTitle')}</span>
              <input
                value={newTitle}
                onChange={(event) => setNewTitle(event.target.value)}
                placeholder={t('wikiTitlePlaceholder')}
                required
              />
            </label>
            <label>
              <span>{t('wikiTags')}</span>
              <input
                value={newTags}
                onChange={(event) => setNewTags(event.target.value)}
                placeholder={t('wikiTagsPlaceholder')}
              />
            </label>
            <label>
              <span>{t('wikiContent')}</span>
              <textarea
                value={newContent}
                onChange={(event) => setNewContent(event.target.value)}
                placeholder={t('wikiContentPlaceholder')}
                rows={6}
              />
            </label>
            <label>
              <span>{t('wikiReason')}</span>
              <input
                value={newReason}
                onChange={(event) => setNewReason(event.target.value)}
                placeholder={t('wikiReasonPlaceholder')}
              />
            </label>
            <button type="submit" disabled={action === 'create' || !newPath.trim() || !newTitle.trim()}>
              {action === 'create' ? t('wikiCreating') : t('wikiCreate')}
            </button>
          </form>
        )}

        <div className="knowledge-list" aria-label={t('wikiPages')}>
          {loading && pages.length === 0 ? (
            <p className="knowledge-list-note">{t('wikiLoading')}</p>
          ) : pages.length === 0 ? (
            <p className="knowledge-list-note">{t('wikiEmpty')}</p>
          ) : pages.map((candidate) => (
            <button
              type="button"
              key={candidate.path}
              className={selectedPath === candidate.path ? 'active' : ''}
              onClick={() => setSelectedPath(candidate.path)}
            >
              <strong>{candidate.title || candidate.path}</strong>
              <span>{candidate.path}</span>
              <small>v{candidate.version} · {formatDate(candidate.updatedAt)}</small>
            </button>
          ))}
        </div>
      </aside>

      <section className="knowledge-editor wiki-editor">
        {error && <div className="workspace-error" role="alert">{error}</div>}
        {!page ? (
          <div className="knowledge-empty-state compact">
            <BookOpen size={26} aria-hidden="true" />
            <h2>{t('wikiSelectPage')}</h2>
            <p>{t('wikiSelectPageDescription')}</p>
          </div>
        ) : (
          <>
            <header className="knowledge-editor-header">
              <div>
                <span className="workspace-eyebrow">{page.path}</span>
                <h2>{displayedTitle}</h2>
                <p>v{historicalRevision?.version ?? page.version} · {formatDate(
                  historicalRevision?.createdAt ?? page.updatedAt,
                )}</p>
              </div>
              <div>
                <label className="revision-select">
                  <Clock3 size={14} aria-hidden="true" />
                  <span className="sr-only">{t('wikiRevision')}</span>
                  <select
                    aria-label={t('wikiRevision')}
                    value={selectedVersion}
                    onChange={(event) => {
                      const value = event.target.value
                      setSelectedVersion(value === 'current' ? 'current' : Number(value))
                      setEditing(false)
                    }}
                  >
                    <option value="current">{t('wikiCurrent')}</option>
                    {revisions.map((revision) => (
                      <option key={revision.version} value={revision.version}>
                        v{revision.version} · {formatDate(revision.createdAt)}
                      </option>
                    ))}
                  </select>
                </label>
                {historicalRevision && (
                  <button
                    type="button"
                    onClick={() => revertRevision(historicalRevision.version)}
                    disabled={action !== null}
                  >
                    <RotateCcw size={14} aria-hidden="true" />
                    {t('wikiRevert')}
                  </button>
                )}
                {!historicalRevision && (
                  <button type="button" onClick={() => setEditing((current) => !current)}>
                    {editing
                      ? <Eye size={14} aria-hidden="true" />
                      : <Pencil size={14} aria-hidden="true" />}
                    {editing ? t('wikiPreview') : t('wikiEdit')}
                  </button>
                )}
              </div>
            </header>

            {editing ? (
              <form className="knowledge-edit-form wiki-edit-form" onSubmit={savePage}>
                <div className="wiki-field-row">
                  <label>
                    <span>{t('wikiPageTitle')}</span>
                    <input
                      value={draftTitle}
                      onChange={(event) => setDraftTitle(event.target.value)}
                      required
                    />
                  </label>
                  <label>
                    <span>{t('wikiTags')}</span>
                    <input
                      value={draftTags}
                      onChange={(event) => setDraftTags(event.target.value)}
                    />
                  </label>
                </div>
                <label className="editor-body">
                  <span>{t('wikiContent')}</span>
                  <textarea
                    value={draftContent}
                    onChange={(event) => setDraftContent(event.target.value)}
                    spellCheck
                  />
                </label>
                <div className="wiki-save-row">
                  <label>
                    <span>{t('wikiReason')}</span>
                    <input
                      value={draftReason}
                      onChange={(event) => setDraftReason(event.target.value)}
                      placeholder={t('wikiReasonPlaceholder')}
                    />
                  </label>
                  <button type="submit" className="primary" disabled={action !== null || !draftTitle.trim()}>
                    <Save size={14} aria-hidden="true" />
                    {action === 'save' ? t('wikiSaving') : t('wikiSave')}
                  </button>
                </div>
              </form>
            ) : (
              <div className="wiki-preview">
                <div className="wiki-tags">
                  {displayedTags.map((candidate) => <span key={candidate}>{candidate}</span>)}
                </div>
                <article className="local-markdown">
                  <MarkdownRenderer>{displayedContent}</MarkdownRenderer>
                </article>
                {!historicalRevision && (
                  <section className="wiki-backlinks" aria-label={t('wikiBacklinks')}>
                    <h3><Link2 size={14} aria-hidden="true" />{t('wikiBacklinks')}</h3>
                    {backlinks.length === 0 ? (
                      <p>{t('wikiNoBacklinks')}</p>
                    ) : (
                      backlinks.map((backlink) => (
                        <button
                          type="button"
                          key={backlink.path}
                          onClick={() => setSelectedPath(backlink.path)}
                        >
                          <strong>[[{backlink.title || backlink.path}]]</strong>
                          <span>{backlink.path} · v{backlink.version}</span>
                        </button>
                      ))
                    )}
                  </section>
                )}
              </div>
            )}
          </>
        )}
      </section>
    </div>
  )
}

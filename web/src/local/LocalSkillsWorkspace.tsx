import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type FormEvent,
} from 'react'
import {
  FileCode2,
  PackagePlus,
  RefreshCw,
  RotateCw,
  Trash2,
  X,
} from 'lucide-react'
import {
  localApi,
  type Agent,
  type AgentSkill,
  type MachineSkillDoctor,
  type RuntimeSkillCompatibility,
  type SkillFileEntry,
  type SkillLibraryEntry,
} from './api'
import { LocalSelect } from './LocalSelect'
import type { LocalCopyKey } from './localization'

interface LocalSkillsWorkspaceProps {
  agents: Agent[]
  t: (key: LocalCopyKey, values?: Record<string, string | number>) => string
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`
}

function compatibilityLabel(
  compatibility: RuntimeSkillCompatibility | undefined,
  t: LocalSkillsWorkspaceProps['t'],
): string {
  switch (compatibility) {
    case 'supported':
      return t('skillsCompatibilitySupported')
    case 'uncertain':
      return t('skillsCompatibilityUncertain')
    case 'unsupported':
      return t('skillsCompatibilityUnsupported')
    default:
      return t('skillsCompatibilityUnknown')
  }
}

export function LocalSkillsWorkspace({ agents, t }: LocalSkillsWorkspaceProps) {
  const [catalog, setCatalog] = useState<SkillLibraryEntry[]>([])
  const [compatibility, setCompatibility] = useState<Record<string, RuntimeSkillCompatibility>>({})
  const [doctor, setDoctor] = useState<MachineSkillDoctor | null>(null)
  const [selectedAgentId, setSelectedAgentId] = useState('')
  const [agentSkills, setAgentSkills] = useState<AgentSkill[]>([])
  const [selectedLibraryId, setSelectedLibraryId] = useState('')
  const [source, setSource] = useState('')
  const [subPath, setSubPath] = useState('')
  const [importName, setImportName] = useState('')
  const [loadingCatalog, setLoadingCatalog] = useState(true)
  const [loadingAgent, setLoadingAgent] = useState(false)
  const [agentSkillQuery, setAgentSkillQuery] = useState('')
  const [action, setAction] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [viewingSkill, setViewingSkill] = useState<AgentSkill | null>(null)
  const [skillFiles, setSkillFiles] = useState<SkillFileEntry[]>([])
  const [selectedFile, setSelectedFile] = useState('')
  const [fileContent, setFileContent] = useState('')
  const [fileBinary, setFileBinary] = useState(false)

  const selectedAgent = useMemo(
    () => agents.find((agent) => agent.id === selectedAgentId) ?? null,
    [agents, selectedAgentId],
  )
  const selectedLibrary = useMemo(
    () => catalog.find((entry) => entry.id === selectedLibraryId) ?? null,
    [catalog, selectedLibraryId],
  )
  const agentOptions = useMemo(
    () => agents.map((agent) => ({
      value: agent.id,
      label: agent.name,
      meta: agent.runtime,
    })),
    [agents],
  )
  const installedLibraryIds = useMemo(
    () => new Set(agentSkills.flatMap((skill) => skill.libraryId ? [skill.libraryId] : [])),
    [agentSkills],
  )
  const visibleAgentSkills = useMemo(() => {
    const query = agentSkillQuery.trim().toLocaleLowerCase()
    if (!query) return agentSkills
    return agentSkills.filter((skill) => (
      skill.name.toLocaleLowerCase().includes(query)
      || skill.displayName?.toLocaleLowerCase().includes(query)
      || skill.description?.toLocaleLowerCase().includes(query)
      || skill.state.toLocaleLowerCase().includes(query)
      || skill.type.toLocaleLowerCase().includes(query)
    ))
  }, [agentSkillQuery, agentSkills])

  const refreshCatalog = useCallback(async () => {
    const response = await localApi.listSkillLibrary()
    setCatalog(response.entries)
    setSelectedLibraryId((current) => (
      response.entries.some((entry) => entry.id === current)
        ? current
        : response.entries[0]?.id ?? ''
    ))
  }, [])

  const refreshDoctor = useCallback(async () => {
    setDoctor(await localApi.inspectMachineSkills())
  }, [])

  const refreshAgentSkills = useCallback(async (agentId: string) => {
    if (!agentId) {
      setAgentSkills([])
      return
    }
    setLoadingAgent(true)
    try {
      const response = await localApi.listAgentSkills(agentId)
      setAgentSkills(response.skills)
    } finally {
      setLoadingAgent(false)
    }
  }, [])

  useEffect(() => {
    if (selectedAgentId && agents.some((agent) => agent.id === selectedAgentId)) return
    setSelectedAgentId(agents[0]?.id ?? '')
  }, [agents, selectedAgentId])

  useEffect(() => {
    let cancelled = false
    setLoadingCatalog(true)
    Promise.all([
      localApi.listSkillLibrary(),
      localApi.listSkillCompatibility(),
      localApi.inspectMachineSkills(),
    ])
      .then(([library, nextCompatibility, nextDoctor]) => {
        if (cancelled) return
        setCatalog(library.entries)
        setSelectedLibraryId((current) => current || library.entries[0]?.id || '')
        setCompatibility(nextCompatibility)
        setDoctor(nextDoctor)
      })
      .catch((nextError: unknown) => {
        if (!cancelled) {
          setError(nextError instanceof Error ? nextError.message : t('skillsLoadError'))
        }
      })
      .finally(() => {
        if (!cancelled) setLoadingCatalog(false)
      })
    return () => {
      cancelled = true
    }
  }, [t])

  useEffect(() => {
    let cancelled = false
    if (!selectedAgentId) {
      setAgentSkills([])
      return
    }
    setLoadingAgent(true)
    localApi.listAgentSkills(selectedAgentId)
      .then((response) => {
        if (!cancelled) setAgentSkills(response.skills)
      })
      .catch((nextError: unknown) => {
        if (!cancelled) {
          setError(nextError instanceof Error ? nextError.message : t('skillsLoadError'))
        }
      })
      .finally(() => {
        if (!cancelled) setLoadingAgent(false)
      })
    return () => {
      cancelled = true
    }
  }, [selectedAgentId, t])

  const runAction = useCallback(async (key: string, task: () => Promise<void>) => {
    setAction(key)
    setError(null)
    try {
      await task()
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : t('skillsActionError'))
    } finally {
      setAction(null)
    }
  }, [t])

  async function importLibrary(event: FormEvent) {
    event.preventDefault()
    const url = source.trim()
    if (!url) return
    await runAction('import', async () => {
      await localApi.importSkillLibrary({
        url,
        subPath: subPath.trim() || undefined,
        name: importName.trim() || undefined,
      })
      setSource('')
      setSubPath('')
      setImportName('')
      await refreshCatalog()
    })
  }

  function installLibrary(libraryId: string) {
    if (!selectedAgentId) return
    void runAction(`install:${libraryId}`, async () => {
      await localApi.installAgentSkill(selectedAgentId, libraryId)
      await Promise.all([
        refreshCatalog(),
        refreshAgentSkills(selectedAgentId),
        refreshDoctor(),
      ])
    })
  }

  function uninstallSkill(skill: AgentSkill) {
    if (!selectedAgentId || !skill.installId) return
    void runAction(`uninstall:${skill.installId}`, async () => {
      await localApi.uninstallAgentSkill(selectedAgentId, skill.installId!)
      if (viewingSkill?.installId === skill.installId) closeViewer()
      await Promise.all([
        refreshCatalog(),
        refreshAgentSkills(selectedAgentId),
        refreshDoctor(),
      ])
    })
  }

  function reinstallLibrary(libraryId: string) {
    void runAction(`reinstall:${libraryId}`, async () => {
      await localApi.reinstallSkillLibrary(libraryId)
      await Promise.all([
        refreshCatalog(),
        selectedAgentId ? refreshAgentSkills(selectedAgentId) : Promise.resolve(),
        refreshDoctor(),
      ])
    })
  }

  function deleteLibrary(library: SkillLibraryEntry) {
    if (!window.confirm(t('skillsDeleteConfirm', { name: library.displayName || library.name }))) {
      return
    }
    void runAction(`delete:${library.id}`, async () => {
      await localApi.deleteSkillLibrary(library.id)
      await Promise.all([
        refreshCatalog(),
        selectedAgentId ? refreshAgentSkills(selectedAgentId) : Promise.resolve(),
        refreshDoctor(),
      ])
    })
  }

  const readFile = useCallback(async (
    agentId: string,
    installId: string,
    relativePath: string,
  ) => {
    setSelectedFile(relativePath)
    setFileContent('')
    setFileBinary(false)
    const response = await localApi.readAgentSkillFile(agentId, installId, relativePath)
    setFileContent(response.content)
    setFileBinary(response.binary)
  }, [])

  function openViewer(skill: AgentSkill) {
    if (!selectedAgentId || !skill.installId) return
    void runAction(`view:${skill.installId}`, async () => {
      const response = await localApi.listAgentSkillFiles(selectedAgentId, skill.installId!)
      const firstFile = response.files.find((file) => file.name === 'SKILL.md' && !file.isDir)
        ?? response.files.find((file) => !file.isDir)
      setViewingSkill(skill)
      setSkillFiles(response.files)
      if (firstFile) {
        await readFile(selectedAgentId, skill.installId!, firstFile.name)
      } else {
        setSelectedFile('')
        setFileContent('')
        setFileBinary(false)
      }
    })
  }

  function closeViewer() {
    setViewingSkill(null)
    setSkillFiles([])
    setSelectedFile('')
    setFileContent('')
    setFileBinary(false)
  }

  const selectedCompatibility = selectedAgent
    ? compatibility[selectedAgent.runtime]
    : undefined
  const installDisabled = !selectedAgent || selectedCompatibility === 'unsupported'
  const selectedInventory = doctor?.agents.find((inventory) => (
    inventory.agentId === selectedAgentId
  )) ?? null

  return (
    <section className="local-skills-workspace" aria-label={t('skillsWorkspace')}>
      <header className="workspace-heading">
        <div>
          <span className="eyebrow">{t('skillsEyebrow')}</span>
          <h1>{t('skillsTitle')}</h1>
          <p>{t('skillsDescription')}</p>
        </div>
        <button
          type="button"
          className="icon-action"
          aria-label={t('refresh')}
          onClick={() => void runAction('refresh', async () => {
            await Promise.all([
              refreshCatalog(),
              selectedAgentId ? refreshAgentSkills(selectedAgentId) : Promise.resolve(),
              refreshDoctor(),
            ])
          })}
          disabled={action === 'refresh'}
        >
          <RefreshCw size={15} aria-hidden="true" />
          {t('refresh')}
        </button>
      </header>

      {error && (
        <div className="workspace-error" role="alert">
          <span>{error}</span>
          <button type="button" onClick={() => setError(null)}>{t('dismiss')}</button>
        </div>
      )}

      <section className="skill-diagnostics" aria-labelledby="skill-inventory-title">
        <div className="workspace-section-title">
          <div>
            <h2 id="skill-inventory-title">{t('skillsInventoryTitle')}</h2>
            <p>{t('skillsInventoryDescription')}</p>
          </div>
          {doctor && (
            <span className={`doctor-status ${doctor.summary.status}`}>
              {doctor.summary.status === 'ok'
                ? t('skillsDoctorOk')
                : doctor.summary.status === 'warning'
                  ? t('skillsDoctorWarning')
                  : t('skillsDoctorError')}
            </span>
          )}
        </div>
        <p className="skill-evidence-note">{t('skillsEvidenceNotice')}</p>
        {doctor && (
          <div className="runtime-skill-matrix-wrap">
            <table className="runtime-skill-matrix">
              <thead>
                <tr>
                  <th>{t('skillsRuntime')}</th>
                  <th>{t('skillsCompatibility')}</th>
                  <th>{t('skillsAgents')}</th>
                  <th>{t('skillsDiscovered')}</th>
                  <th>{t('skillsIssues')}</th>
                  <th>{t('skillsEvidence')}</th>
                </tr>
              </thead>
              <tbody>
                {doctor.runtimes.map((runtime) => (
                  <tr key={runtime.runtime}>
                    <th scope="row">{runtime.runtime}</th>
                    <td>{compatibilityLabel(runtime.compatibility, t)}</td>
                    <td>{runtime.agentCount}</td>
                    <td>{runtime.skillCount}</td>
                    <td>{runtime.issueCount}</td>
                    <td>{runtime.evidenceSources.join(', ') || t('skillsNoEvidence')}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        {selectedInventory && (
          <details className="agent-skill-doctor" open={selectedInventory.issues.length > 0}>
            <summary>{t('skillsAgentDoctor', { agent: selectedInventory.agentName })}</summary>
            <div className="agent-skill-doctor-grid">
              <div>
                <h3>{t('skillsSearchPaths')}</h3>
                <ul>
                  {selectedInventory.searchPaths.map((path) => (
                    <li key={`${path.scope}:${path.path}`}>
                      <code>{path.path}</code>
                      <span>{path.scope} · {path.exists && path.readable ? t('skillsPathReady') : t('skillsPathUnavailable')}</span>
                      {path.symlink && <span>{t('skillsSymlink')}</span>}
                    </li>
                  ))}
                </ul>
              </div>
              <div>
                <h3>{t('skillsIssues')}</h3>
                {selectedInventory.issues.length === 0
                  ? <p>{t('skillsNoIssues')}</p>
                  : (
                    <ul>
                      {selectedInventory.issues.map((issue, index) => (
                        <li key={`${issue.code}:${issue.path ?? index}`} className={issue.severity}>
                          <strong>{issue.code}</strong>
                          <span>{issue.message}</span>
                          {issue.path && <code>{issue.path}</code>}
                        </li>
                      ))}
                    </ul>
                  )}
              </div>
            </div>
          </details>
        )}
      </section>

      <div className="skills-layout">
        <section className="skills-catalog-pane">
          <div className="workspace-section-title">
            <div>
              <h2>{t('skillsCatalog')}</h2>
              <p>{t('skillsCatalogDescription')}</p>
            </div>
            <span>{catalog.length}</span>
          </div>

          <form className="skill-import-form" onSubmit={importLibrary}>
            <label htmlFor="skill-source">{t('skillsSource')}</label>
            <input
              id="skill-source"
              value={source}
              onChange={(event) => setSource(event.target.value)}
              placeholder={t('skillsSourcePlaceholder')}
            />
            <div className="skill-import-options">
              <div>
                <label htmlFor="skill-subpath">{t('skillsSubpath')}</label>
                <input
                  id="skill-subpath"
                  value={subPath}
                  onChange={(event) => setSubPath(event.target.value)}
                  placeholder={t('skillsSubpathPlaceholder')}
                />
              </div>
              <div>
                <label htmlFor="skill-name">{t('skillsNameOverride')}</label>
                <input
                  id="skill-name"
                  value={importName}
                  onChange={(event) => setImportName(event.target.value)}
                  placeholder={t('skillsNamePlaceholder')}
                />
              </div>
            </div>
            <button
              type="submit"
              className="primary-action"
              disabled={!source.trim() || action === 'import'}
            >
              <PackagePlus size={15} aria-hidden="true" />
              {action === 'import' ? t('skillsImporting') : t('skillsImport')}
            </button>
          </form>

          <div className="skill-catalog-list" aria-busy={loadingCatalog}>
            {loadingCatalog && <p className="quiet-copy">{t('skillsLoadingCatalog')}</p>}
            {!loadingCatalog && catalog.length === 0 && (
              <div className="workspace-empty">
                <span>01</span>
                <h3>{t('skillsEmptyCatalog')}</h3>
                <p>{t('skillsEmptyCatalogDescription')}</p>
              </div>
            )}
            {catalog.map((library) => {
              const installed = installedLibraryIds.has(library.id)
              const selected = library.id === selectedLibraryId
              return (
                <article
                  className={`skill-catalog-card${selected ? ' selected' : ''}`}
                  key={library.id}
                >
                  <button
                    type="button"
                    className="skill-card-main"
                    onClick={() => setSelectedLibraryId(library.id)}
                  >
                    <div>
                      <strong>{library.displayName || library.name}</strong>
                      <code>/{library.name}</code>
                    </div>
                    <p>{library.description || t('skillsNoDescription')}</p>
                    <dl>
                      <div><dt>{t('skillsFiles')}</dt><dd>{library.fileCount}</dd></div>
                      <div><dt>{t('skillsSize')}</dt><dd>{formatBytes(library.totalBytes)}</dd></div>
                      <div><dt>{t('skillsInUse')}</dt><dd>{library.inUseCount}</dd></div>
                    </dl>
                    <small title={library.sourceUrl}>{library.sourceUrl}</small>
                  </button>
                  <div className="skill-card-actions">
                    <button
                      type="button"
                      onClick={() => reinstallLibrary(library.id)}
                      disabled={action === `reinstall:${library.id}`}
                    >
                      <RotateCw size={13} aria-hidden="true" />
                      {t('skillsReinstall')}
                    </button>
                    <button
                      type="button"
                      onClick={() => installLibrary(library.id)}
                      disabled={
                        installed
                        || installDisabled
                        || action === `install:${library.id}`
                      }
                    >
                      <PackagePlus size={13} aria-hidden="true" />
                      {installed ? t('skillsInstalled') : t('skillsInstall')}
                    </button>
                    <button
                      type="button"
                      className="danger-action"
                      aria-label={t('skillsDelete')}
                      onClick={() => deleteLibrary(library)}
                      disabled={action === `delete:${library.id}`}
                    >
                      <Trash2 size={13} aria-hidden="true" />
                    </button>
                  </div>
                </article>
              )
            })}
          </div>
        </section>

        <section className="skills-agent-pane">
          <div className="workspace-section-title">
            <div>
              <h2>{t('skillsAgentWorkspace')}</h2>
              <p>{t('skillsAgentWorkspaceDescription')}</p>
            </div>
            <span>{agentSkillQuery ? `${visibleAgentSkills.length}/${agentSkills.length}` : agentSkills.length}</span>
          </div>

          <div className="agent-skill-selector">
            <label htmlFor="skills-agent">{t('skillsAgent')}</label>
            <LocalSelect
              id="skills-agent"
              ariaLabel={t('skillsAgent')}
              value={selectedAgentId}
              options={agentOptions}
              onChange={setSelectedAgentId}
              disabled={agents.length === 0}
              placeholder={t('skillsSelectAgent')}
            />
            {selectedAgent && (
              <div className={`compatibility-note ${selectedCompatibility ?? 'unknown'}`}>
                <span>{selectedAgent.runtime}</span>
                <strong>{compatibilityLabel(selectedCompatibility, t)}</strong>
              </div>
            )}
          </div>

          {!selectedAgent && (
            <div className="workspace-empty compact">
              <span>02</span>
              <h3>{t('skillsNoAgent')}</h3>
              <p>{t('skillsNoAgentDescription')}</p>
            </div>
          )}

          {selectedAgent && loadingAgent && (
            <p className="quiet-copy">{t('skillsLoadingAgent')}</p>
          )}

          {selectedAgent && !loadingAgent && agentSkills.length === 0 && (
            <div className="workspace-empty compact">
              <span>03</span>
              <h3>{t('skillsEmptyAgent')}</h3>
              <p>{t('skillsEmptyAgentDescription')}</p>
            </div>
          )}

          {selectedAgent && agentSkills.length > 0 && (
            <label className="agent-skill-filter" htmlFor="agent-skill-filter">
              <span>{t('skillsFilter')}</span>
              <input
                id="agent-skill-filter"
                type="search"
                value={agentSkillQuery}
                onChange={(event) => setAgentSkillQuery(event.target.value)}
                placeholder={t('skillsFilterPlaceholder')}
              />
            </label>
          )}

          {viewingSkill && selectedAgentId && viewingSkill.installId && (
            <section className="skill-file-viewer">
              <header>
                <div>
                  <span className="eyebrow">{t('skillsFileBrowser')}</span>
                  <h3>{viewingSkill.displayName || viewingSkill.name}</h3>
                </div>
                <button type="button" aria-label={t('close')} onClick={closeViewer}>
                  <X size={15} aria-hidden="true" />
                </button>
              </header>
              <div className="skill-file-layout">
                <nav aria-label={t('skillsFiles')}>
                  {skillFiles.map((file) => (
                    <button
                      type="button"
                      key={file.name}
                      className={file.name === selectedFile ? 'active' : ''}
                      disabled={file.isDir}
                      onClick={() => {
                        if (!file.isDir) {
                          void runAction(`file:${file.name}`, () => readFile(
                            selectedAgentId,
                            viewingSkill.installId!,
                            file.name,
                          ))
                        }
                      }}
                    >
                      <span>{file.isDir ? '▸' : '·'}</span>
                      {file.name}
                    </button>
                  ))}
                </nav>
                <div className="skill-file-content">
                  {fileBinary
                    ? <p>{t('skillsBinaryFile')}</p>
                    : <pre>{fileContent || t('skillsSelectFile')}</pre>}
                </div>
              </div>
            </section>
          )}

          {selectedAgent && !loadingAgent && agentSkills.length > 0 && visibleAgentSkills.length === 0 && (
            <div className="workspace-empty compact">
              <span>04</span>
              <h3>{t('skillsNoMatches')}</h3>
              <p>{t('skillsNoMatchesDescription')}</p>
            </div>
          )}

          <div className="agent-skill-list">
            {visibleAgentSkills.map((skill) => (
              <article className="agent-skill-card" key={`${skill.type}/${skill.name}/${skill.installPath}`}>
                <div className="agent-skill-heading">
                  <div>
                    <strong>{skill.displayName || skill.name}</strong>
                    <code>{skill.type}</code>
                  </div>
                  <span className={`skill-state ${skill.state}`}>{skill.state}</span>
                </div>
                {skill.description && <p>{skill.description}</p>}
                <small>{skill.state === 'broken' ? skill.installPath : skill.path}</small>
                <div className="skill-evidence-row">
                  <span>{skill.presence}</span>
                  <span>{skill.runtime} · {skill.scope}</span>
                  <span>{skill.evidence.source}</span>
                  {skill.resolvedPath && skill.resolvedPath !== skill.sourcePath && (
                    <span title={skill.resolvedPath}>{t('skillsSymlink')}</span>
                  )}
                  {skill.valid === false && <span className="error">invalid</span>}
                  {skill.enabled === false && <span className="warning">{t('skillsDisabled')}</span>}
                  {skill.shadowed && <span className="warning">shadowed</span>}
                  {skill.duplicate && !skill.shadowed && <span className="warning">duplicate</span>}
                </div>
                <div className="agent-skill-actions">
                  {skill.installId && skill.state !== 'broken' && (
                    <button
                      type="button"
                      onClick={() => openViewer(skill)}
                      disabled={action === `view:${skill.installId}`}
                    >
                      <FileCode2 size={13} aria-hidden="true" />
                      {t('skillsViewFiles')}
                    </button>
                  )}
                  {skill.installId && (
                    <button
                      type="button"
                      className="danger-action"
                      onClick={() => uninstallSkill(skill)}
                      disabled={action === `uninstall:${skill.installId}`}
                    >
                      <Trash2 size={13} aria-hidden="true" />
                      {skill.state === 'broken' ? t('skillsRemoveRecord') : t('skillsUninstall')}
                    </button>
                  )}
                </div>
              </article>
            ))}
          </div>

          {selectedLibrary && selectedAgent && !installedLibraryIds.has(selectedLibrary.id) && (
            <div className="selected-library-hint">
              <span>{t('skillsSelectedLibrary')}</span>
              <strong>{selectedLibrary.displayName || selectedLibrary.name}</strong>
              <button
                type="button"
                onClick={() => installLibrary(selectedLibrary.id)}
                disabled={installDisabled || action === `install:${selectedLibrary.id}`}
              >
                {t('skillsInstallTo', { agent: selectedAgent.name })}
              </button>
            </div>
          )}
        </section>
      </div>
    </section>
  )
}

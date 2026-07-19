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
  ShieldCheck,
  Trash2,
  X,
} from 'lucide-react'
import {
  localApi,
  type Agent,
  type AgentSkill,
  type MachineSkillDoctor,
  type RuntimeSkillCompatibility,
  type SkillGovernanceBinding,
  type SkillGovernanceEffectiveDesired,
  type SkillGovernanceLockPreviewResponse,
  type SkillGovernanceObservation,
  type SkillGovernancePlanPreviewResponse,
  type SkillGovernanceProfile,
  type SkillGovernanceScope,
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

type GovernanceTab = 'profiles' | 'lock' | 'plan' | 'evidence'

function shortHash(value: string | undefined): string {
  return value ? value.slice(0, 12) : 'unknown'
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
  const [issueSeverity, setIssueSeverity] = useState('all')
  const [issueRuntime, setIssueRuntime] = useState('all')
  const [issueScope, setIssueScope] = useState('all')
  const [action, setAction] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [viewingSkill, setViewingSkill] = useState<AgentSkill | null>(null)
  const [skillFiles, setSkillFiles] = useState<SkillFileEntry[]>([])
  const [selectedFile, setSelectedFile] = useState('')
  const [fileContent, setFileContent] = useState('')
  const [fileBinary, setFileBinary] = useState(false)
  const [governanceTab, setGovernanceTab] = useState<GovernanceTab>('profiles')
  const [governanceProfiles, setGovernanceProfiles] = useState<SkillGovernanceProfile[]>([])
  const [governanceBindings, setGovernanceBindings] = useState<SkillGovernanceBinding[]>([])
  const [governanceEvidence, setGovernanceEvidence] = useState<SkillGovernanceObservation | null>(null)
  const [effectiveDesired, setEffectiveDesired] = useState<SkillGovernanceEffectiveDesired | null>(null)
  const [lockPreview, setLockPreview] = useState<SkillGovernanceLockPreviewResponse | null>(null)
  const [planPreview, setPlanPreview] = useState<SkillGovernancePlanPreviewResponse | null>(null)
  const [profileName, setProfileName] = useState('default-governance-profile')
  const [profileDescription, setProfileDescription] = useState('Safe default profile with no desired skills yet.')
  const [bindingProfileId, setBindingProfileId] = useState('')
  const [bindingScope, setBindingScope] = useState<SkillGovernanceScope>('machine')
  const [bindingScopeId, setBindingScopeId] = useState('machine')
  const [governanceWorkspaceId, setGovernanceWorkspaceId] = useState('')
  const [governanceAgentId, setGovernanceAgentId] = useState('')
  const [driftFilter, setDriftFilter] = useState('all')
  const [actionFilter, setActionFilter] = useState('all')
  const [governanceRuntimeFilter, setGovernanceRuntimeFilter] = useState('all')
  const [governanceScopeFilter, setGovernanceScopeFilter] = useState('all')

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
  const governanceTarget = useMemo(() => ({
    workspaceId: governanceWorkspaceId.trim() || undefined,
    agentId: governanceAgentId.trim() || selectedAgentId || undefined,
  }), [governanceAgentId, governanceWorkspaceId, selectedAgentId])
  const governanceRequest = useMemo(() => ({
    scope: bindingScope,
    scopeId: bindingScope === 'machine' ? 'machine' : bindingScopeId.trim(),
    workspaceId: governanceTarget.workspaceId,
    agentId: governanceTarget.agentId,
    force: true,
  }), [bindingScope, bindingScopeId, governanceTarget])
  const governanceRuntimeOptions = useMemo(() => {
    const runtimes = new Set<string>()
    governanceEvidence?.skills.forEach((skill) => runtimes.add(skill.runtime))
    lockPreview?.drift.forEach((drift) => runtimes.add(drift.runtime))
    planPreview?.preview.content.actions.forEach((item) => runtimes.add(item.runtime))
    return [...runtimes].sort()
  }, [governanceEvidence, lockPreview, planPreview])
  const visibleDrift = useMemo(() => (
    (lockPreview?.drift ?? []).filter((drift) => (
      (driftFilter === 'all' || drift.kind === driftFilter)
      && (governanceRuntimeFilter === 'all' || drift.runtime === governanceRuntimeFilter)
      && (governanceScopeFilter === 'all' || drift.scope === governanceScopeFilter)
    ))
  ), [driftFilter, governanceRuntimeFilter, governanceScopeFilter, lockPreview])
  const visiblePlanActions = useMemo(() => (
    (planPreview?.preview.content.actions ?? []).filter((item) => (
      (actionFilter === 'all' || item.action === actionFilter)
      && (governanceRuntimeFilter === 'all' || item.runtime === governanceRuntimeFilter)
      && (governanceScopeFilter === 'all' || item.scope === governanceScopeFilter)
    ))
  ), [actionFilter, governanceRuntimeFilter, governanceScopeFilter, planPreview])
  const visibleEvidence = useMemo(() => (
    (governanceEvidence?.skills ?? []).filter((skill) => (
      (governanceRuntimeFilter === 'all' || skill.runtime === governanceRuntimeFilter)
      && (governanceScopeFilter === 'all' || skill.scope === governanceScopeFilter)
    ))
  ), [governanceEvidence, governanceRuntimeFilter, governanceScopeFilter])

  const refreshCatalog = useCallback(async () => {
    const response = await localApi.listSkillLibrary()
    setCatalog(response.entries)
    setSelectedLibraryId((current) => (
      response.entries.some((entry) => entry.id === current)
        ? current
        : response.entries[0]?.id ?? ''
    ))
  }, [])

  const refreshDoctor = useCallback(async (force = false) => {
    setDoctor(await localApi.inspectMachineSkills(force))
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

  const refreshGovernance = useCallback(async (force = false) => {
    const target = {
      workspaceId: governanceWorkspaceId.trim() || undefined,
      agentId: governanceAgentId.trim() || selectedAgentId || undefined,
    }
    const [profiles, bindings, evidence, desired] = await Promise.all([
      localApi.listGovernanceProfiles(),
      localApi.listGovernanceBindings(),
      localApi.getGovernanceEvidence(force),
      localApi.getGovernanceEffectiveDesired(target),
    ])
    setGovernanceProfiles(profiles)
    setGovernanceBindings(bindings)
    setGovernanceEvidence(evidence)
    setEffectiveDesired(desired)
    setBindingProfileId((current) => (
      profiles.some((profile) => profile.id === current) ? current : profiles[0]?.id ?? ''
    ))
  }, [governanceAgentId, governanceWorkspaceId, selectedAgentId])

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

  useEffect(() => {
    if (governanceAgentId || !selectedAgentId) return
    setGovernanceAgentId(selectedAgentId)
  }, [governanceAgentId, selectedAgentId])

  useEffect(() => {
    if (bindingScope === 'machine') {
      setBindingScopeId('machine')
    } else if (bindingScope === 'agent' && selectedAgentId && (!bindingScopeId || bindingScopeId === 'machine')) {
      setBindingScopeId(selectedAgentId)
    } else if (bindingScope === 'workspace' && bindingScopeId === 'machine') {
      setBindingScopeId('')
    }
  }, [bindingScope, bindingScopeId, selectedAgentId])

  useEffect(() => {
    let cancelled = false
    refreshGovernance()
      .catch((nextError: unknown) => {
        if (!cancelled) {
          setError(nextError instanceof Error ? nextError.message : t('skillsLoadError'))
        }
      })
    return () => {
      cancelled = true
    }
  }, [refreshGovernance, t])

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
        refreshDoctor(true),
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
        refreshDoctor(true),
      ])
    })
  }

  function reinstallLibrary(libraryId: string) {
    void runAction(`reinstall:${libraryId}`, async () => {
      await localApi.reinstallSkillLibrary(libraryId)
      await Promise.all([
        refreshCatalog(),
        selectedAgentId ? refreshAgentSkills(selectedAgentId) : Promise.resolve(),
        refreshDoctor(true),
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
        refreshDoctor(true),
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

  async function createGovernanceProfile(event: FormEvent) {
    event.preventDefault()
    const name = profileName.trim()
    if (!name) return
    await runAction('governance:create-profile', async () => {
      const profile = await localApi.createGovernanceProfile({
        schemaVersion: 1,
        name,
        description: profileDescription.trim(),
        skills: [],
      })
      setBindingProfileId(profile.id)
      await refreshGovernance(true)
    })
  }

  async function bindGovernanceProfile(event: FormEvent) {
    event.preventDefault()
    if (!bindingProfileId || !governanceRequest.scopeId) return
    await runAction('governance:bind-profile', async () => {
      await localApi.bindGovernanceProfile({
        profileId: bindingProfileId,
        scope: bindingScope,
        scopeId: governanceRequest.scopeId,
      })
      await refreshGovernance(true)
    })
  }

  function previewGovernanceLock() {
    if (!governanceRequest.scopeId) return
    void runAction('governance:preview-lock', async () => {
      const preview = await localApi.previewGovernanceLock(governanceRequest)
      setLockPreview(preview)
      setGovernanceTab('lock')
      await refreshGovernance(true)
    })
  }

  function previewGovernancePlan() {
    if (!governanceRequest.scopeId) return
    void runAction('governance:preview-plan', async () => {
      const preview = await localApi.previewGovernancePlan(governanceRequest)
      setPlanPreview(preview)
      setGovernanceTab('plan')
      await refreshGovernance(true)
    })
  }

  const selectedCompatibility = selectedAgent
    ? compatibility[selectedAgent.runtime]
    : undefined
  const installDisabled = !selectedAgent || selectedCompatibility === 'unsupported'
  const selectedInventory = doctor?.agents.find((inventory) => (
    inventory.agentId === selectedAgentId
  )) ?? null
  const visibleIssues = useMemo(() => {
    if (!doctor) return []
    const candidates = [
      ...doctor.runtimes.flatMap((runtime) => (runtime.issues ?? []).map((issue) => ({
        ...issue,
        runtime: runtime.runtime,
        scope: 'machine' as const,
      }))),
      ...doctor.agents.flatMap((inventory) => inventory.issues.map((issue) => ({
        ...issue,
        runtime: inventory.runtime,
        scope: 'agent' as const,
      }))),
      ...(doctor.diagnostics ?? []).map((diagnostic) => ({
        fingerprint: diagnostic.fingerprint,
        code: diagnostic.errorType,
        severity: 'error' as const,
        message: diagnostic.message,
        relatedPaths: [],
        runtime: diagnostic.runtime,
        scope: diagnostic.subject === 'agent' ? 'agent' as const : 'machine' as const,
      })),
    ].filter((issue) => (
      (issueSeverity === 'all' || issue.severity === issueSeverity)
      && (issueRuntime === 'all' || issue.runtime === issueRuntime)
      && (issueScope === 'all' || issue.scope === issueScope)
    ))
    const grouped = new Map<string, (typeof candidates)[number]>()
    for (const issue of candidates) {
      const existing = grouped.get(issue.fingerprint)
      if (!existing) {
        grouped.set(issue.fingerprint, issue)
        continue
      }
      existing.relatedPaths = [...new Set([
        ...(existing.relatedPaths ?? []),
        ...(issue.relatedPaths ?? []),
      ])]
    }
    return [...grouped.values()]
  }, [doctor, issueRuntime, issueScope, issueSeverity])

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
              refreshDoctor(true),
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

      <section className="skill-governance" aria-labelledby="skill-governance-title">
        <div className="workspace-section-title">
          <div>
            <h2 id="skill-governance-title">{t('skillsGovernanceTitle')}</h2>
            <p>{t('skillsGovernanceDescription')}</p>
          </div>
          <span>{t('skillsGovernanceDryRun')}</span>
        </div>
        <div className="governance-status-row">
          <span className="governance-pill">{t('skillsGovernancePreviewOnly')}</span>
          <span className="governance-pill warning">{t('skillsGovernanceUnknownSession')}</span>
          {planPreview?.plan.status === 'approved' && !planPreview.applied && (
            <span className="governance-pill warning">{t('skillsGovernanceApprovedNotApplied')}</span>
          )}
          {lockPreview && (
            <span className={`governance-pill ${lockPreview.lockfileChanged ? 'warning' : 'ok'}`}>
              {lockPreview.lockfileChanged
                ? t('skillsGovernanceLockChanged')
                : t('skillsGovernanceLockUnchanged')}
            </span>
          )}
          {lockPreview?.writesRealDirectories
            ? <span className="governance-pill danger">{t('skillsGovernanceWritesRealDirectories')}</span>
            : <span className="governance-pill ok">{t('skillsGovernancePreviewOnly')}</span>}
          {lockPreview?.lockfileBoundary && (
            <span className="governance-pill">{lockPreview.lockfileBoundary}</span>
          )}
        </div>
        <div className="governance-tabs" role="tablist" aria-label={t('skillsGovernanceTitle')}>
          {([
            ['profiles', t('skillsGovernanceProfiles')],
            ['lock', t('skillsGovernanceLockDrift')],
            ['plan', t('skillsGovernancePlanPreview')],
            ['evidence', t('skillsGovernanceEvidence')],
          ] as const).map(([key, label]) => (
            <button
              key={key}
              type="button"
              role="tab"
              aria-selected={governanceTab === key}
              className={governanceTab === key ? 'active' : ''}
              onClick={() => setGovernanceTab(key)}
            >
              {label}
            </button>
          ))}
        </div>

        {governanceTab === 'profiles' && (
          <div className="governance-grid">
            <form className="governance-form" onSubmit={createGovernanceProfile}>
              <label htmlFor="governance-profile-name">{t('skillsGovernanceProfileName')}</label>
              <input
                id="governance-profile-name"
                value={profileName}
                onChange={(event) => setProfileName(event.target.value)}
              />
              <label htmlFor="governance-profile-description">{t('skillsGovernanceProfileDescription')}</label>
              <input
                id="governance-profile-description"
                value={profileDescription}
                onChange={(event) => setProfileDescription(event.target.value)}
              />
              <p>{t('skillsGovernanceProfileSafeDefault')}</p>
              <button
                type="submit"
                className="primary-action"
                disabled={!profileName.trim() || action === 'governance:create-profile'}
              >
                <ShieldCheck size={15} aria-hidden="true" />
                {action === 'governance:create-profile'
                  ? t('skillsGovernanceCreatingProfile')
                  : t('skillsGovernanceCreateProfile')}
              </button>
            </form>
            <form className="governance-form" onSubmit={bindGovernanceProfile}>
              <label htmlFor="governance-profile-select">{t('skillsGovernanceBindProfile')}</label>
              <select
                id="governance-profile-select"
                value={bindingProfileId}
                onChange={(event) => setBindingProfileId(event.target.value)}
              >
                <option value="">{t('skillsGovernanceNoProfiles')}</option>
                {governanceProfiles.map((profile) => (
                  <option key={profile.id} value={profile.id}>
                    {profile.name} · v{profile.version}
                  </option>
                ))}
              </select>
              <label htmlFor="governance-binding-scope">{t('skillsGovernanceBindingScope')}</label>
              <select
                id="governance-binding-scope"
                value={bindingScope}
                onChange={(event) => setBindingScope(event.target.value as SkillGovernanceScope)}
              >
                <option value="machine">{t('skillsGovernanceMachine')}</option>
                <option value="workspace">{t('skillsGovernanceWorkspace')}</option>
                <option value="agent">{t('skillsGovernanceAgent')}</option>
              </select>
              <label htmlFor="governance-scope-id">{t('skillsGovernanceScopeId')}</label>
              <input
                id="governance-scope-id"
                value={bindingScopeId}
                onChange={(event) => setBindingScopeId(event.target.value)}
                placeholder={t('skillsGovernanceScopeIdPlaceholder')}
                disabled={bindingScope === 'machine'}
              />
              <button
                type="submit"
                disabled={!bindingProfileId || !governanceRequest.scopeId || action === 'governance:bind-profile'}
              >
                {t('skillsGovernanceBindProfile')}
              </button>
            </form>
            <div className="governance-list">
              <h3>{t('skillsGovernanceProfiles')}</h3>
              {governanceProfiles.length === 0 && <p>{t('skillsGovernanceNoProfiles')}</p>}
              {governanceProfiles.map((profile) => (
                <article key={profile.id}>
                  <strong>{profile.name}</strong>
                  <span>v{profile.version} · {profile.skills.length} desired</span>
                  {profile.description && <p>{profile.description}</p>}
                </article>
              ))}
            </div>
            <div className="governance-list">
              <h3>{t('skillsGovernanceBindProfile')}</h3>
              {governanceBindings.length === 0 && <p>{t('skillsGovernanceNoBindings')}</p>}
              {governanceBindings.map((binding) => (
                <article key={binding.id}>
                  <strong>{binding.scope}:{binding.scopeId}</strong>
                  <span>{shortHash(binding.profileId)} · v{binding.version}</span>
                </article>
              ))}
            </div>
          </div>
        )}

        {governanceTab !== 'profiles' && (
          <div className="governance-controls">
            <label>
              <span>{t('skillsGovernanceTargetWorkspace')}</span>
              <input
                value={governanceWorkspaceId}
                onChange={(event) => setGovernanceWorkspaceId(event.target.value)}
                placeholder="workspace id"
              />
            </label>
            <label>
              <span>{t('skillsGovernanceTargetAgent')}</span>
              <input
                value={governanceAgentId}
                onChange={(event) => setGovernanceAgentId(event.target.value)}
                placeholder={selectedAgentId || 'agent id'}
              />
            </label>
            <label>
              <span>{t('skillsGovernanceRuntimeFilter')}</span>
              <select value={governanceRuntimeFilter} onChange={(event) => setGovernanceRuntimeFilter(event.target.value)}>
                <option value="all">{t('skillsAll')}</option>
                {governanceRuntimeOptions.map((runtime) => <option key={runtime} value={runtime}>{runtime}</option>)}
              </select>
            </label>
            <label>
              <span>{t('skillsGovernanceScopeFilter')}</span>
              <select value={governanceScopeFilter} onChange={(event) => setGovernanceScopeFilter(event.target.value)}>
                <option value="all">{t('skillsAll')}</option>
                <option value="machine">{t('skillsGovernanceMachine')}</option>
                <option value="workspace">{t('skillsGovernanceWorkspace')}</option>
                <option value="agent">{t('skillsGovernanceAgent')}</option>
              </select>
            </label>
            {governanceTab === 'lock' && (
              <label>
                <span>{t('skillsGovernanceDriftFilter')}</span>
                <select value={driftFilter} onChange={(event) => setDriftFilter(event.target.value)}>
                  <option value="all">{t('skillsAll')}</option>
                  {[...new Set((lockPreview?.drift ?? []).map((drift) => drift.kind))].map((kind) => (
                    <option key={kind} value={kind}>{kind}</option>
                  ))}
                </select>
              </label>
            )}
            {governanceTab === 'plan' && (
              <label>
                <span>{t('skillsGovernanceActionFilter')}</span>
                <select value={actionFilter} onChange={(event) => setActionFilter(event.target.value)}>
                  <option value="all">{t('skillsAll')}</option>
                  {[...new Set((planPreview?.preview.content.actions ?? []).map((item) => item.action))].map((kind) => (
                    <option key={kind} value={kind}>{kind}</option>
                  ))}
                </select>
              </label>
            )}
            <button type="button" onClick={previewGovernanceLock} disabled={!governanceRequest.scopeId || action === 'governance:preview-lock'}>
              {t('skillsGovernancePreviewLock')}
            </button>
            <button type="button" onClick={previewGovernancePlan} disabled={!governanceRequest.scopeId || action === 'governance:preview-plan'}>
              {t('skillsGovernancePreviewPlan')}
            </button>
            <button
              type="button"
              onClick={() => void runAction('governance:evidence', () => refreshGovernance(true))}
              disabled={action === 'governance:evidence'}
            >
              {t('skillsGovernanceRefreshEvidence')}
            </button>
          </div>
        )}

        {governanceTab === 'lock' && (
          <div className="governance-list">
            <h3>{t('skillsGovernanceLockDrift')} · {shortHash(lockPreview?.preview.lockfileHash)}</h3>
            {visibleDrift.length === 0 && <p>{t('skillsGovernanceNoDrift')}</p>}
            {visibleDrift.map((drift) => (
              <article key={drift.fingerprint}>
                <strong>{drift.kind} · {drift.logicalIdentity}</strong>
                <span>{drift.runtime} · {drift.scope}</span>
                <p>{drift.reason}</p>
                {(drift.expected || drift.actual) && <code>{drift.expected ?? 'unknown'} → {drift.actual ?? 'unknown'}</code>}
              </article>
            ))}
          </div>
        )}

        {governanceTab === 'plan' && (
          <div className="governance-list">
            <h3>
              {t('skillsGovernancePlanPreview')} · {planPreview?.preview.dryRun ? t('skillsGovernanceDryRun') : t('skillsGovernanceStatus')}
            </h3>
            {planPreview && (
              <div className="governance-status-row">
                <span className="governance-pill">{planPreview.plan.status}</span>
                <span className="governance-pill">{planPreview.applied ? t('skillsGovernanceApplied') : t('skillsGovernanceNotApplied')}</span>
                {!planPreview.applied && planPreview.plan.status === 'approved' && (
                  <span className="governance-pill warning">{t('skillsGovernanceApprovedNotApplied')}</span>
                )}
              </div>
            )}
            {visiblePlanActions.length === 0 && <p>{t('skillsGovernanceNoActions')}</p>}
            {visiblePlanActions.map((item) => (
              <article key={`${item.action}:${item.skillFingerprint}`}>
                <strong>{item.action} · {item.target}</strong>
                <span>{item.runtime} · {item.scope} · {item.risk}</span>
                <p>{item.reason}</p>
                <code>{item.before} → {item.after}</code>
              </article>
            ))}
            {planPreview?.plan.plan.staleReasons?.length ? (
              <div>
                <h3>{t('skillsGovernanceStaleReasons')}</h3>
                {planPreview.plan.plan.staleReasons.map((reason) => <p key={reason}>{reason}</p>)}
              </div>
            ) : null}
          </div>
        )}

        {governanceTab === 'evidence' && (
          <div className="governance-list">
            <h3>{t('skillsGovernanceEvidence')} · {shortHash(governanceEvidence?.snapshotHash)}</h3>
            {effectiveDesired && (
              <div className="governance-status-row">
                <span className="governance-pill">{t('skillsGovernanceDesired')}: {effectiveDesired.skills.length}</span>
                <span className={`governance-pill ${effectiveDesired.conflicts.length > 0 ? 'danger' : 'ok'}`}>
                  {t('skillsGovernanceConflicts')}: {effectiveDesired.conflicts.length}
                </span>
              </div>
            )}
            {visibleEvidence.length === 0 && <p>{t('skillsGovernanceNoEvidenceRows')}</p>}
            {visibleEvidence.map((skill) => (
              <article key={skill.fingerprint}>
                <strong>{skill.logicalIdentity}</strong>
                <span>{skill.runtime} · {skill.scope} · {skill.evidenceStatus}</span>
                <p>
                  {skill.sessionEffective === 'unknown'
                    ? t('skillsGovernanceUnknownSession')
                    : `${skill.sessionEffective}: ${skill.sessionReason}`}
                </p>
                <code>{skill.evidenceSource} · {skill.destination ?? skill.fingerprint}</code>
              </article>
            ))}
          </div>
        )}
      </section>

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
          <p className="skill-snapshot-note">
            {t('skillsSnapshotStatus', {
              status: doctor.cacheStatus ?? 'fresh',
              observedAt: doctor.observedAt
                ? new Date(doctor.observedAt).toLocaleString()
                : t('skillsNoEvidence'),
            })}
          </p>
        )}
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
        {doctor && (
          <div className="skill-issue-browser">
            <div className="skill-issue-filters">
              <label>
                <span>{t('skillsSeverity')}</span>
                <select value={issueSeverity} onChange={(event) => setIssueSeverity(event.target.value)}>
                  <option value="all">{t('skillsAll')}</option>
                  <option value="error">{t('skillsDoctorError')}</option>
                  <option value="warning">{t('skillsDoctorWarning')}</option>
                </select>
              </label>
              <label>
                <span>{t('skillsRuntime')}</span>
                <select value={issueRuntime} onChange={(event) => setIssueRuntime(event.target.value)}>
                  <option value="all">{t('skillsAll')}</option>
                  {doctor.runtimes.map((runtime) => (
                    <option key={runtime.runtime} value={runtime.runtime}>{runtime.runtime}</option>
                  ))}
                </select>
              </label>
              <label>
                <span>{t('skillsScope')}</span>
                <select value={issueScope} onChange={(event) => setIssueScope(event.target.value)}>
                  <option value="all">{t('skillsAll')}</option>
                  <option value="machine">{t('skillsMachineScope')}</option>
                  <option value="agent">{t('skillsAgentScope')}</option>
                </select>
              </label>
            </div>
            {visibleIssues.length === 0
              ? <p>{t('skillsNoIssues')}</p>
              : (
                <ul className="skill-grouped-issues">
                  {visibleIssues.map((issue) => (
                    <li key={issue.fingerprint} className={issue.severity}>
                      <strong>{issue.code}</strong>
                      <span>{issue.runtime} · {issue.scope}</span>
                      <p>{issue.message}</p>
                      {issue.relatedPaths?.map((path) => <code key={path}>{path}</code>)}
                    </li>
                  ))}
                </ul>
              )}
          </div>
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

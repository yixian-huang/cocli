import { useCallback, useEffect, useMemo, useState } from 'react'
import { RefreshCw } from 'lucide-react'
import {
  localApi,
  type McpApplyRun,
  type McpDoctorReport,
  type McpEffectiveDesiredState,
  type McpPlanAction,
  type McpPlanView,
  type McpProfile,
  type McpProfileBinding,
  type McpStateSummary,
  type ObservedMcpInstance,
} from './api'
import type { LocalCopyKey } from './localization'

interface LocalMcpWorkspaceProps {
  t: (key: LocalCopyKey, values?: Record<string, string | number>) => string
}

function stateMark(value: boolean | undefined): string {
  if (value === true) return '✓'
  if (value === false) return '×'
  return '·'
}

function observationTitle(observation: ObservedMcpInstance): string {
  return [
    `configured=${observation.configured}`,
    `loaded=${observation.loaded ?? 'unknown'}`,
    `enabled=${observation.enabled ?? 'unknown'}`,
    `approved=${observation.approved ?? 'unknown'}`,
    `authenticated=${observation.authenticated ?? 'unknown'}`,
    `healthy=${observation.healthy ?? 'unknown'}`,
    `sessionVisible=${observation.currentSessionVisible ?? 'unknown'}`,
    `invoked=${observation.invoked ?? 'unknown'}`,
  ].join(', ')
}

function targetLabel(target: { targetType: string; targetId: string }): string {
  return `${target.targetType}:${target.targetId}`
}

function summaryLabel(summary: McpStateSummary): string {
  const parts = [
    `configured=${summary.configured ?? 'unknown'}`,
    `enabled=${summary.enabled ?? 'unknown'}`,
  ]
  if (summary.endpointFingerprint) parts.push(`fingerprint=${summary.endpointFingerprint}`)
  if (summary.allowTools.length > 0) parts.push(`allow=${summary.allowTools.join(',')}`)
  if (summary.denyTools.length > 0) parts.push(`deny=${summary.denyTools.join(',')}`)
  if (summary.approvalMode) parts.push(`approval=${summary.approvalMode}`)
  parts.push(`secretRefs=${summary.secretRefCount}`)
  return parts.join(', ')
}

function actionEvidence(action: McpPlanAction): string {
  if (action.evidence.length === 0) return 'unknown evidence'
  return action.evidence.map((evidence) => `${evidence.source}: ${evidence.detail}`).join(' · ')
}

function isExecutableAction(action: McpPlanAction): boolean {
  return !action.blocked
    && action.kind !== 'approval_required'
    && action.kind !== 'authentication_required'
    && action.kind !== 'manual_unsupported'
}

export function LocalMcpWorkspace({ t }: LocalMcpWorkspaceProps) {
  const [report, setReport] = useState<McpDoctorReport | null>(null)
  const [profiles, setProfiles] = useState<McpProfile[]>([])
  const [bindings, setBindings] = useState<McpProfileBinding[]>([])
  const [effective, setEffective] = useState<McpEffectiveDesiredState | null>(null)
  const [planView, setPlanView] = useState<McpPlanView | null>(null)
  const [applyRun, setApplyRun] = useState<McpApplyRun | null>(null)
  const [loading, setLoading] = useState(true)
  const [planning, setPlanning] = useState(false)
  const [decisionBusy, setDecisionBusy] = useState(false)
  const [applying, setApplying] = useState(false)
  const [rollingBack, setRollingBack] = useState(false)
  const [confirmHighRisk, setConfirmHighRisk] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const [nextReport, nextProfiles, nextBindings, nextEffective] = await Promise.all([
        localApi.inspectMachineMcp(),
        localApi.listMcpProfiles(),
        localApi.listMcpProfileBindings(),
        localApi.getMcpEffectiveDesiredState(),
      ])
      setReport(nextReport)
      setProfiles(nextProfiles.profiles)
      setBindings(nextBindings.bindings)
      setEffective(nextEffective)
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : t('mcpLoadError'))
    } finally {
      setLoading(false)
    }
  }, [t])

  useEffect(() => {
    void refresh()
  }, [refresh])

  const runtimes = useMemo(() => {
    const names = new Set<string>()
    report?.inventory.observations.forEach((item) => names.add(item.runtime))
    report?.inventory.diagnostics.forEach((item) => {
      if (item.runtime !== 'aggregate' && item.runtime !== 'machine') names.add(item.runtime)
    })
    return [...names].sort()
  }, [report])

  const findObservation = (runtime: string, serverId: string) =>
    report?.inventory.observations.find(
      (item) => item.runtime === runtime && item.serverId === serverId,
    )

  const generatePlan = async () => {
    setPlanning(true)
    setError(null)
    setApplyRun(null)
    setConfirmHighRisk(false)
    try {
      setPlanView(await localApi.createMcpPlan())
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : t('mcpPlanError'))
    } finally {
      setPlanning(false)
    }
  }

  const decidePlan = async (decision: 'approve' | 'reject') => {
    if (!planView) return
    setDecisionBusy(true)
    setError(null)
    try {
      const next = decision === 'approve'
        ? await localApi.approveMcpPlan(planView.plan.id, planView.plan.planHash)
        : await localApi.rejectMcpPlan(planView.plan.id, planView.plan.planHash, 'Rejected from desktop preview')
      setPlanView(next)
      setConfirmHighRisk(false)
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : t('mcpPlanError'))
    } finally {
      setDecisionBusy(false)
    }
  }

  const highRiskPlan = planView?.plan.actions.some((action) => action.risk === 'high' || action.risk === 'critical') ?? false
  const executableActionCount = planView?.plan.actions.filter(isExecutableAction).length ?? 0
  const canApplyPlan = planView?.approvalStatus === 'approved'
    && planView.staleReasons.length === 0
    && planView.approvedButNotApplied
    && (!highRiskPlan || confirmHighRisk)

  const applyPlan = async () => {
    if (!planView) return
    setApplying(true)
    setError(null)
    try {
      const next = await localApi.applyMcpPlan(planView.plan.id, {
        planHash: planView.plan.planHash,
        observationHash: planView.plan.observationHash,
        configHash: planView.plan.configHash,
        confirmHighRisk,
      })
      setApplyRun(next.run)
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : t('mcpApplyError'))
    } finally {
      setApplying(false)
    }
  }

  const rollbackApplyRun = async () => {
    if (!applyRun) return
    setRollingBack(true)
    setError(null)
    try {
      const next = await localApi.rollbackMcpApplyRun(applyRun.id)
      setApplyRun(next.run)
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : t('mcpRollbackError'))
    } finally {
      setRollingBack(false)
    }
  }

  return (
    <section className="local-mcp-workspace" aria-label={t('mcpWorkspace')}>
      <header className="workspace-heading">
        <div>
          <span className="eyebrow">{t('mcpEyebrow')}</span>
          <h1>{t('mcpTitle')}</h1>
          <p>{t('mcpDescription')}</p>
        </div>
        <button className="icon-action" type="button" onClick={() => void refresh()} disabled={loading}>
          <RefreshCw size={14} aria-hidden="true" />
          {t('refresh')}
        </button>
      </header>

      {error && <p className="form-error" role="alert">{error}</p>}
      {loading && !report && <p className="quiet-copy">{t('mcpLoading')}</p>}

      {report && (
        <>
          <p className="mcp-zero-write-notice">{t('mcpZeroWriteNotice')}</p>

          <div className="mcp-summary-grid">
            <article className={report.summary.status}>
              <span>{t('mcpDoctorStatus')}</span>
              <strong>{report.summary.status}</strong>
            </article>
            <article><span>{t('mcpServers')}</span><strong>{report.summary.serverCount}</strong></article>
            <article><span>{t('mcpObservations')}</span><strong>{report.summary.observationCount}</strong></article>
            <article><span>{t('mcpFindings')}</span><strong>{report.summary.diagnosticCount}</strong></article>
          </div>

          <section className="mcp-panel" aria-labelledby="mcp-matrix-title">
            <div className="workspace-section-title">
              <div>
                <h2 id="mcp-matrix-title">{t('mcpMatrix')}</h2>
                <p>{t('mcpMatrixDescription')}</p>
              </div>
            </div>
            <div className="skill-inventory-table-wrap">
              <table className="skill-inventory-table mcp-matrix">
                <thead>
                  <tr>
                    <th>{t('mcpRuntime')}</th>
                    {report.inventory.servers.map((server) => (
                      <th key={server.id} title={server.endpointFingerprint}>{server.canonicalName}</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {runtimes.map((runtime) => (
                    <tr key={runtime}>
                      <th scope="row">{runtime}</th>
                      {report.inventory.servers.map((server) => {
                        const item = findObservation(runtime, server.id)
                        return (
                          <td key={server.id} title={item ? observationTitle(item) : t('mcpNotObserved')}>
                            {item ? (
                              <span className={`mcp-state ${item.healthy === false ? 'error' : item.approved === false ? 'warning' : 'ok'}`}>
                                D{stateMark(item.discoverable)} C{stateMark(item.configured)} L{stateMark(item.loaded)} E{stateMark(item.enabled)} P{stateMark(item.approved)} A{stateMark(item.authenticated)} H{stateMark(item.healthy)} S{stateMark(item.currentSessionVisible)} I{stateMark(item.invoked)}
                              </span>
                            ) : '—'}
                          </td>
                        )
                      })}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </section>

          <section className="mcp-panel" aria-labelledby="mcp-evidence-title">
            <div className="workspace-section-title">
              <div>
                <h2 id="mcp-evidence-title">{t('mcpEvidence')}</h2>
                <p>{t('mcpEvidenceDescription')}</p>
              </div>
            </div>
            <div className="mcp-observation-list">
              {report.inventory.observations.map((item) => (
                <article key={`${item.runtime}-${item.serverId}-${item.alias}`}>
                  <header>
                    <strong>{item.runtime} · {item.alias}</strong>
                    <time dateTime={item.observedAt}>{new Date(item.observedAt).toLocaleString()}</time>
                  </header>
                  <p>{item.evidence.map((evidence) => `${evidence.source}: ${evidence.detail}`).join(' · ')}</p>
                  <small>{observationTitle(item)}{item.toolCount !== undefined ? `, tools=${item.toolCount}` : ''}</small>
                </article>
              ))}
            </div>
          </section>

          <section className="mcp-panel" aria-labelledby="mcp-findings-title">
            <div className="workspace-section-title"><div><h2 id="mcp-findings-title">{t('mcpFindings')}</h2></div></div>
            {report.inventory.diagnostics.length === 0 ? <p>{t('mcpNoFindings')}</p> : (
              <ul className="mcp-finding-list">
                {report.inventory.diagnostics.map((finding, index) => (
                  <li className={finding.severity} key={`${finding.runtime}-${finding.code}-${index}`}>
                    <strong>{finding.runtime} · {finding.code}</strong>
                    <span>{finding.message}</span>
                    <time dateTime={finding.observedAt}>{new Date(finding.observedAt).toLocaleString()}</time>
                  </li>
                ))}
              </ul>
            )}
          </section>

          <section className="mcp-panel" aria-labelledby="mcp-profiles-title">
            <div className="workspace-section-title">
              <div>
                <h2 id="mcp-profiles-title">{t('mcpProfiles')}</h2>
                <p>{t('mcpProfilesDescription')}</p>
              </div>
            </div>
            {profiles.length === 0 ? <p>{t('mcpNoProfiles')}</p> : (
              <div className="mcp-profile-list">
                {profiles.map((profile) => {
                  const profileBindings = bindings.filter((binding) => binding.profileId === profile.id)
                  return (
                    <article key={profile.id}>
                      <header>
                        <div>
                          <strong>{profile.name}</strong>
                          <small>v{profile.version} · {profile.servers.length} {t('mcpProfileServers')}</small>
                        </div>
                        <span>{profileBindings.map((binding) => targetLabel(binding.target)).join(' · ') || t('mcpNoBindings')}</span>
                      </header>
                      {profile.description && <p>{profile.description}</p>}
                      <ul>
                        {profile.servers.map((server) => (
                          <li key={`${profile.id}-${server.runtime}-${server.serverId}`}>
                            <strong>{server.runtime} · {server.alias}</strong>
                            <span>{server.desiredEnabled ? t('mcpDesiredEnabled') : t('mcpDesiredDisabled')} · {server.approvalMode} · {server.riskOverride ?? 'low'} · secretRefs={server.secretRefs.length}</span>
                          </li>
                        ))}
                      </ul>
                    </article>
                  )
                })}
              </div>
            )}
          </section>

          <section className="mcp-panel" aria-labelledby="mcp-effective-title">
            <div className="workspace-section-title">
              <div>
                <h2 id="mcp-effective-title">{t('mcpEffectiveDesired')}</h2>
                <p>{t('mcpEffectiveDescription')}</p>
              </div>
            </div>
            {effective ? (
              <>
                <div className="mcp-effective-meta">
                  <span>{t('mcpEffectiveTarget')}: {effective.target.machineId}{effective.target.workspaceId ? ` / ${effective.target.workspaceId}` : ''}{effective.target.agentId ? ` / ${effective.target.agentId}` : ''}</span>
                  <span>{t('mcpConflicts')}: {effective.conflicts.length}</span>
                </div>
                {effective.conflicts.length > 0 && (
                  <ul className="mcp-conflict-list">
                    {effective.conflicts.map((conflict) => (
                      <li key={`${conflict.runtime}-${conflict.serverId}-${conflict.precedence}`}>
                        <strong>{conflict.runtime} · {conflict.serverId}</strong>
                        <span>{conflict.precedence} · {conflict.reason} · {conflict.profileIds.join(', ')}</span>
                      </li>
                    ))}
                  </ul>
                )}
                <div className="mcp-profile-list">
                  {effective.servers.map((server) => (
                    <article key={`${server.runtime}-${server.serverId}`}>
                      <header>
                        <div>
                          <strong>{server.runtime} · {server.alias}</strong>
                          <small>{server.inheritedFrom} · {server.sourceProfileNames.join(', ')}</small>
                        </div>
                        <span>{server.highRiskContext ? t('mcpHighRiskContext') : t('mcpStandardContext')}</span>
                      </header>
                      <p>{server.desiredEnabled ? t('mcpDesiredEnabled') : t('mcpDesiredDisabled')} · allow={server.allowTools.join(', ') || 'none'} · deny={server.denyTools.join(', ') || 'none'} · secretRefs={server.secretRefs.length}</p>
                    </article>
                  ))}
                </div>
                <ul className="mcp-resolution-list">
                  {effective.resolution.map((resolution) => (
                    <li key={resolution.bindingId}>
                      <strong>{resolution.applied ? t('mcpAppliedResolution') : t('mcpSkippedResolution')}</strong>
                      <span>{resolution.profileName} -&gt; {targetLabel(resolution.target)} · {resolution.reason}</span>
                    </li>
                  ))}
                </ul>
              </>
            ) : <p>{t('mcpNoEffectiveDesired')}</p>}
          </section>

          <section className="mcp-panel" aria-labelledby="mcp-plan-title">
            <div className="workspace-section-title">
              <div>
                <h2 id="mcp-plan-title">{t('mcpPlanPreview')}</h2>
                <p>{t('mcpPlanDescription')}</p>
              </div>
              <button className="secondary-action" type="button" onClick={() => void generatePlan()} disabled={planning}>
                {planning ? t('mcpPlanning') : t('mcpGeneratePlan')}
              </button>
            </div>
            {planView ? (
              <>
                <div className="mcp-plan-status">
                  <span>{t('mcpApprovalStatus')}: {planView.approvalStatus}</span>
                  <span>{t('mcpPlanHash')}: {planView.plan.planHash}</span>
                  <span>{planView.plan.dryRun ? t('mcpDryRun') : t('mcpNotDryRun')}</span>
                  {planView.decision?.expiresAt && <span>{t('mcpApprovalExpires')}: {new Date(planView.decision.expiresAt).toLocaleString()}</span>}
                  {planView.approvedButNotApplied && <strong>{t('mcpApprovedNotApplied')}</strong>}
                </div>
                <p className="mcp-apply-boundary">{t('mcpApplyBoundary')}</p>
                {planView.staleReasons.length > 0 && (
                  <ul className="mcp-conflict-list">
                    {planView.staleReasons.map((reason) => <li key={reason}>{reason}</li>)}
                  </ul>
                )}
                <div className="mcp-plan-actions">
                  {planView.plan.actions.map((action) => (
                    <article className={`risk-${action.risk}`} key={`${action.kind}-${action.runtime}-${action.serverId}-${action.serverFingerprint}`}>
                      <header>
                        <div>
                          <strong>{action.kind} · {action.runtime} · {action.scope}</strong>
                          <small>{action.target} · {action.serverId} · {action.serverFingerprint}</small>
                        </div>
                        <span>{action.risk}{action.blocked ? ` · ${t('mcpBlocked')}` : ''}</span>
                      </header>
                      <p>{action.reason}</p>
                      <dl>
                        <div><dt>{t('mcpBefore')}</dt><dd>{summaryLabel(action.before)}</dd></div>
                        <div><dt>{t('mcpAfter')}</dt><dd>{summaryLabel(action.after)}</dd></div>
                        <div><dt>{t('mcpEvidence')}</dt><dd>{actionEvidence(action)}</dd></div>
                        <div><dt>{t('mcpExpectedHashes')}</dt><dd>{action.expectedSourceHash ?? 'unknown'} / {action.expectedSchemaHash ?? 'unknown'}</dd></div>
                        <div><dt>{t('mcpExecution')}</dt><dd>{isExecutableAction(action) ? t('mcpExecutableAction') : t('mcpNonExecutableAction')}</dd></div>
                      </dl>
                    </article>
                  ))}
                </div>
                <div className="mcp-plan-controls">
                  <button type="button" className="primary-action" onClick={() => void decidePlan('approve')} disabled={decisionBusy || planView.approvalStatus === 'approved'}>
                    {t('mcpApprovePlan')}
                  </button>
                  <button type="button" className="secondary-action" onClick={() => void decidePlan('reject')} disabled={decisionBusy || planView.approvalStatus === 'rejected'}>
                    {t('mcpRejectPlan')}
                  </button>
                </div>
                <div className="mcp-apply-controls" aria-label={t('mcpApplyControls')}>
                  <div>
                    <strong>{t('mcpApplyPreview')}</strong>
                    <p>{t('mcpApplyPreviewDetail', { count: executableActionCount })}</p>
                  </div>
                  {highRiskPlan && (
                    <label className="mcp-high-risk-confirm">
                      <input
                        type="checkbox"
                        checked={confirmHighRisk}
                        onChange={(event) => setConfirmHighRisk(event.currentTarget.checked)}
                      />
                      <span>{t('mcpConfirmHighRisk')}</span>
                    </label>
                  )}
                  <button type="button" className="primary-action" onClick={() => void applyPlan()} disabled={applying || !canApplyPlan}>
                    {applying ? t('mcpApplying') : t('mcpApplyApprovedPlan')}
                  </button>
                  {planView.approvalStatus !== 'approved' && <span>{t('mcpApplyNeedsApproval')}</span>}
                  {planView.approvalStatus === 'approved' && planView.staleReasons.length > 0 && <span>{t('mcpApplyStale')}</span>}
                </div>
                {applyRun && (
                  <section className="mcp-apply-run" aria-labelledby="mcp-apply-run-title">
                    <div className="workspace-section-title">
                      <div>
                        <h3 id="mcp-apply-run-title">{t('mcpApplyRun')}</h3>
                        <p>{t('mcpApplyRunDescription')}</p>
                      </div>
                      {applyRun.canRollback && (
                        <button type="button" className="secondary-action" onClick={() => void rollbackApplyRun()} disabled={rollingBack}>
                          {rollingBack ? t('mcpRollingBack') : t('mcpRollback')}
                        </button>
                      )}
                    </div>
                    <div className="mcp-plan-status">
                      <span>{t('mcpApplyRunId')}: {applyRun.id}</span>
                      <span>{t('mcpApplyStatus')}: {applyRun.status}</span>
                      <span>{t('mcpPlanHash')}: {applyRun.planHash}</span>
                      <span>{t('mcpExpectedHashes')}: {applyRun.observationHash} / {applyRun.configHash}</span>
                      {applyRun.rollbackStatus && <span>{t('mcpRollbackStatus')}: {applyRun.rollbackStatus}</span>}
                    </div>
                    {applyRun.staleReasons.length > 0 && (
                      <ul className="mcp-conflict-list">
                        {applyRun.staleReasons.map((reason) => <li key={reason}>{reason}</li>)}
                      </ul>
                    )}
                    <div className="mcp-apply-columns">
                      <div>
                        <h4>{t('mcpApplyActions')}</h4>
                        <ul className="mcp-conflict-list">
                          {applyRun.actions.map((action) => (
                            <li key={`${action.actionIndex}-${action.runtime}-${action.serverId}`}>
                              <strong>{action.status} · {action.runtime} · {action.serverId}</strong>
                              <span>{action.reason}{action.backup ? ` · backup=${action.backup.id}` : ''}</span>
                            </li>
                          ))}
                        </ul>
                      </div>
                      <div>
                        <h4>{t('mcpReloadAndVerify')}</h4>
                        <ul className="mcp-conflict-list">
                          {applyRun.reloads.map((reload) => (
                            <li key={`${reload.runtime}-${reload.status}`}>
                              <strong>{reload.status} · {reload.runtime}</strong>
                              <span>{reload.reason}</span>
                            </li>
                          ))}
                          <li>
                            <strong>{t('mcpVerify')}: {applyRun.verification.status}</strong>
                            <span>{applyRun.verification.mismatches.join(' · ') || applyRun.verification.observationHash}</span>
                          </li>
                        </ul>
                      </div>
                    </div>
                    {(applyRun.rollbackActions?.length ?? 0) > 0 && (
                      <div>
                        <h4>{t('mcpRollback')}</h4>
                        <ul className="mcp-conflict-list">
                          {applyRun.rollbackActions?.map((action) => (
                            <li key={`rollback-${action.actionIndex}-${action.runtime}-${action.serverId}`}>
                              <strong>{action.status} · {action.runtime}</strong>
                              <span>{action.reason}</span>
                            </li>
                          ))}
                        </ul>
                      </div>
                    )}
                  </section>
                )}
              </>
            ) : <p>{t('mcpNoPlan')}</p>}
          </section>
        </>
      )}
    </section>
  )
}

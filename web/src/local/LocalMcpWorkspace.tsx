import { useCallback, useEffect, useMemo, useState } from 'react'
import { RefreshCw } from 'lucide-react'
import {
  localApi,
  type McpDoctorReport,
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

export function LocalMcpWorkspace({ t }: LocalMcpWorkspaceProps) {
  const [report, setReport] = useState<McpDoctorReport | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      setReport(await localApi.inspectMachineMcp())
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
        </>
      )}
    </section>
  )
}

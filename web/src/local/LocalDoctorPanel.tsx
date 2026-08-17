import { useCallback, useState } from 'react'
import { AlertTriangle, CheckCircle2, CircleAlert, RefreshCw, Stethoscope } from 'lucide-react'

import { localApi, type DoctorStatus, type MachineDoctorReport } from './api'
import type { LocalCopyKey } from './localization'

interface LocalDoctorPanelProps {
  t: (key: LocalCopyKey, values?: Record<string, string | number>) => string
}

function statusIcon(status: DoctorStatus) {
  if (status === 'error') return <CircleAlert size={17} aria-hidden="true" />
  if (status === 'warning') return <AlertTriangle size={17} aria-hidden="true" />
  return <CheckCircle2 size={17} aria-hidden="true" />
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : 'Unexpected local service error'
}

export function LocalDoctorPanel({ t }: LocalDoctorPanelProps) {
  const [report, setReport] = useState<MachineDoctorReport | null>(null)
  const [pending, setPending] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const runDoctor = useCallback(async () => {
    setPending(true)
    setError(null)
    try {
      setReport(await localApi.inspectMachineDoctor(true))
    } catch (nextError) {
      setError(errorMessage(nextError))
    } finally {
      setPending(false)
    }
  }, [])

  return (
    <article className="settings-panel doctor-panel" aria-labelledby="doctor-settings-title">
      <header className="doctor-panel-header">
        <div>
          <h2 id="doctor-settings-title">{t('doctorTitle')}</h2>
          <p>{t('doctorDescription')}</p>
        </div>
        {report && (
          <span className="doctor-status" data-status={report.summary.status}>
            {statusIcon(report.summary.status)}
            {t(`doctorStatus${report.summary.status[0].toUpperCase()}${report.summary.status.slice(1)}` as LocalCopyKey)}
          </span>
        )}
      </header>

      <div className="settings-panel-actions">
        <button
          className="settings-primary-action"
          type="button"
          onClick={() => void runDoctor()}
          disabled={pending}
        >
          {pending
            ? <RefreshCw className="doctor-spin" size={15} aria-hidden="true" />
            : <Stethoscope size={15} aria-hidden="true" />}
          {pending ? t('doctorRunning') : t('doctorRun')}
        </button>
        <span className="quiet-copy">{t('doctorReadOnly')}</span>
      </div>

      {error && <p className="doctor-error" role="alert">{t('doctorLoadError')} {error}</p>}

      {report && (
        <>
          <div className="doctor-summary-grid" aria-label={t('doctorSummary')}>
            <div>
              <strong>{report.summary.checkCount}</strong>
              <span>{t('doctorChecks')}</span>
            </div>
            <div>
              <strong>{report.summary.errorCount}</strong>
              <span>{t('doctorErrors')}</span>
            </div>
            <div>
              <strong>{report.summary.warningCount}</strong>
              <span>{t('doctorWarnings')}</span>
            </div>
            <div>
              <strong>{report.summary.installedRuntimeCount}/{report.summary.runtimeCount}</strong>
              <span>{t('doctorRuntimes')}</span>
            </div>
          </div>

          <div className="doctor-scope-grid">
            {([
              ['machine', report.machine],
              ['user', report.user],
              ['runtime', report.runtime],
            ] as const).map(([scope, summary]) => (
              <div key={scope} className="doctor-scope-card" data-status={summary.status}>
                <span>{t(`doctorScope${scope[0].toUpperCase()}${scope.slice(1)}` as LocalCopyKey)}</span>
                <strong>{t(`doctorStatus${summary.status[0].toUpperCase()}${summary.status.slice(1)}` as LocalCopyKey)}</strong>
                <small>{summary.errorCount} E · {summary.warningCount} W · {summary.infoCount} I</small>
              </div>
            ))}
          </div>

          <section className="doctor-findings" aria-labelledby="doctor-findings-title">
            <h3 id="doctor-findings-title">{t('doctorFindings')}</h3>
            {report.findings.length === 0 ? (
              <p className="doctor-empty"><CheckCircle2 size={16} aria-hidden="true" />{t('doctorNoFindings')}</p>
            ) : (
              <ul>
                {report.findings.map((finding) => (
                  <li key={finding.id} data-severity={finding.severity}>
                    <div className="doctor-finding-heading">
                      <span>{finding.severity}</span>
                      <code>{finding.domain}/{finding.code}</code>
                      {finding.runtime && <small>{finding.runtime}</small>}
                    </div>
                    <p>{finding.message}</p>
                    {finding.remediation && <p className="doctor-remediation">{t('doctorFix')} {finding.remediation}</p>}
                    {finding.newSessionRequired && <small>{t('doctorNewSession')}</small>}
                    {finding.evidence.length > 0 && (
                      <details>
                        <summary>{t('doctorEvidence')}</summary>
                        <ul>{finding.evidence.map((item) => <li key={item}>{item}</li>)}</ul>
                      </details>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </section>
          <p className="quiet-copy doctor-observed">
            {t('doctorObserved')} {new Date(report.observedAt).toLocaleString()}
          </p>
        </>
      )}
    </article>
  )
}

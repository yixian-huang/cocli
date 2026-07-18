import { Brain } from 'lucide-react'
import type { Agent } from './api'
import { LocalMemoryWorkspace } from './LocalMemoryWorkspace'
import type { LocalCopyKey } from './localization'
import './LocalKnowledgeWorkspace.css'

interface LocalKnowledgeWorkspaceProps {
  agents: Agent[]
  t: (key: LocalCopyKey, values?: Record<string, string | number>) => string
}

export function LocalKnowledgeWorkspace({
  agents,
  t,
}: LocalKnowledgeWorkspaceProps) {
  return (
    <section className="local-knowledge-workspace">
      <header className="knowledge-hero">
        <div>
          <span className="workspace-eyebrow">{t('knowledgeEyebrow')}</span>
          <h1>{t('knowledgeTitle')}</h1>
          <p>{t('knowledgeDescription')}</p>
        </div>
        <div className="knowledge-tabs" aria-label={t('knowledgeView')}>
          <button
            type="button"
            className="active"
            aria-current="page"
          >
            <Brain size={15} aria-hidden="true" />
            {t('knowledgeMemory')}
          </button>
        </div>
      </header>

      <LocalMemoryWorkspace agents={agents} t={t} />
    </section>
  )
}

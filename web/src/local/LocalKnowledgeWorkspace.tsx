import { Brain, BookOpen } from 'lucide-react'
import { useState } from 'react'
import type { Agent, Channel } from './api'
import { LocalMemoryWorkspace } from './LocalMemoryWorkspace'
import { LocalWikiWorkspace } from './LocalWikiWorkspace'
import type { LocalCopyKey } from './localization'
import './LocalKnowledgeWorkspace.css'

interface LocalKnowledgeWorkspaceProps {
  agents: Agent[]
  channels: Channel[]
  t: (key: LocalCopyKey, values?: Record<string, string | number>) => string
}

export function LocalKnowledgeWorkspace({
  agents,
  channels,
  t,
}: LocalKnowledgeWorkspaceProps) {
  const [view, setView] = useState<'memory' | 'wiki'>('memory')

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
            className={view === 'memory' ? 'active' : ''}
            aria-current={view === 'memory' ? 'page' : undefined}
            onClick={() => setView('memory')}
          >
            <Brain size={15} aria-hidden="true" />
            {t('knowledgeMemory')}
          </button>
          <button
            type="button"
            className={view === 'wiki' ? 'active' : ''}
            aria-current={view === 'wiki' ? 'page' : undefined}
            onClick={() => setView('wiki')}
          >
            <BookOpen size={15} aria-hidden="true" />
            {t('knowledgeWiki')}
          </button>
        </div>
      </header>

      {view === 'memory' ? (
        <LocalMemoryWorkspace agents={agents} channels={channels} t={t} />
      ) : (
        <LocalWikiWorkspace t={t} />
      )}
    </section>
  )
}

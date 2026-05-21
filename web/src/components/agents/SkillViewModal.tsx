import { useEffect, useState } from 'react'
import { Modal } from '@/components/ui'
import { MarkdownRenderer } from '@/components/chat/MarkdownRenderer'
import { agentSkills } from '@/api/client'
import { Loader2, File as FileIcon, Folder } from 'lucide-react'
import type { SkillView, SkillFileEntry } from '@/lib/types'

interface Props {
  agentId: string
  skill: SkillView
  onClose: () => void
}

const SKILL_MD = 'SKILL.md'

// Map common script extensions to markdown code fence language tags.
const langByExt: Record<string, string> = {
  '.sh': 'bash',
  '.py': 'python',
  '.js': 'javascript',
  '.ts': 'typescript',
  '.json': 'json',
  '.yaml': 'yaml',
  '.yml': 'yaml',
  '.md': '',
  '.txt': '',
}

function languageFor(name: string): string {
  const dot = name.lastIndexOf('.')
  if (dot < 0) return ''
  return langByExt[name.slice(dot).toLowerCase()] ?? ''
}

export function SkillViewModal({ agentId, skill, onClose }: Props) {
  const [files, setFiles] = useState<SkillFileEntry[]>([])
  const [selected, setSelected] = useState<string>(SKILL_MD)
  const [content, setContent] = useState<string>('')
  const [binary, setBinary] = useState(false)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    if (!skill.installId) return
    let cancelled = false
    agentSkills.listFiles(agentId, skill.installId).then((res) => {
      if (cancelled) return
      setFiles(res.files)
      // Default to SKILL.md if present, otherwise first non-dir file
      const hasSkillMd = res.files.some((f) => f.name === SKILL_MD)
      if (!hasSkillMd) {
        const first = res.files.find((f) => !f.isDir)
        if (first) setSelected(first.name)
      }
    })
    return () => { cancelled = true }
  }, [agentId, skill.installId])

  useEffect(() => {
    if (!skill.installId || !selected) return
    let cancelled = false
    setLoading(true)
    agentSkills.getFile(agentId, skill.installId, selected)
      .then((res) => {
        if (cancelled) return
        setBinary(res.binary)
        setContent(res.content)
      })
      .catch(() => { if (!cancelled) setContent('') })
      .finally(() => { if (!cancelled) setLoading(false) })
    return () => { cancelled = true }
  }, [agentId, skill.installId, selected])

  const markdownText = (() => {
    if (binary) return ''
    if (selected.toLowerCase().endsWith('.md')) return content
    const lang = languageFor(selected)
    return '```' + lang + '\n' + content + '\n```'
  })()

  return (
    <Modal open onClose={onClose} title={skill.displayName || skill.name} size="lg">
      <div className="flex gap-3 h-[70vh]">
        <aside className="w-48 shrink-0 border-r border-border pr-2 overflow-y-auto">
          <ul className="space-y-1 text-xs">
            {files.map((f) => (
              <li key={f.name}>
                <button
                  className={`w-full text-left px-2 py-1 rounded flex items-center gap-1.5 ${
                    selected === f.name ? 'bg-accent' : 'hover:bg-accent/50'
                  }`}
                  onClick={() => !f.isDir && setSelected(f.name)}
                  disabled={f.isDir}
                  aria-current={selected === f.name}
                >
                  {f.isDir ? <Folder className="h-3 w-3" /> : <FileIcon className="h-3 w-3" />}
                  <span className="truncate">{f.name}</span>
                </button>
              </li>
            ))}
          </ul>
        </aside>
        <div className="flex-1 overflow-y-auto">
          {loading ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : binary ? (
            <p className="text-sm text-muted-foreground">binary file, view raw not supported in v1</p>
          ) : (
            <MarkdownRenderer>{markdownText}</MarkdownRenderer>
          )}
        </div>
      </div>
    </Modal>
  )
}

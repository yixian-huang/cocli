import { useEffect, useState } from 'react'
import { useMemoryStore } from '@/stores/memoryStore'
import { MarkdownRenderer } from '@/components/chat/MarkdownRenderer'

type MemoryType = 'user' | 'feedback' | 'project' | 'reference'
const TYPES: ReadonlyArray<MemoryType> = ['user', 'feedback', 'project', 'reference']

interface IndexEntry {
  slug: string          // full slug after the type prefix, e.g. "alice" or "alice_frontend_prefs"
  fullName: string      // "<type>_<slug>" — the display token
  type: MemoryType
  desc: string
}

// parseIndex extracts entries from the MEMORY.md bullet list format:
//   "- [user_alice](user_alice.md) — Alice frontend prefs"
function parseIndex(body: string): IndexEntry[] {
  const out: IndexEntry[] = []
  for (const line of body.split('\n')) {
    const m = line.match(/^- \[([a-z]+)_([a-z0-9_]+)\]\([^)]+\)\s*[—-]\s*(.*)$/)
    if (!m) continue
    const [, type, slug, desc] = m
    if ((TYPES as readonly string[]).includes(type)) {
      out.push({
        slug,
        fullName: `${type}_${slug}`,
        type: type as MemoryType,
        desc: desc.trim(),
      })
    }
  }
  return out
}

interface MemoryTabProps {
  agentId: string
}

export function MemoryTab({ agentId }: MemoryTabProps) {
  const entries = useMemoryStore((s) => s.entries[`agent:${agentId}`])
  const topics = useMemoryStore((s) => s.topics)
  const loadIndex = useMemoryStore((s) => s.loadAgentIndex)
  const loadTopic = useMemoryStore((s) => s.loadAgentTopic)

  const [selected, setSelected] = useState<IndexEntry | null>(null)
  const [filter, setFilter] = useState<MemoryType | 'all'>('all')

  useEffect(() => {
    void loadIndex(agentId)
  }, [agentId, loadIndex])

  const parsed = entries ? parseIndex(entries) : []
  const visible = filter === 'all' ? parsed : parsed.filter((e) => e.type === filter)

  type TopicKey = `agent:${string}:${string}:${string}`
  const topicKey: TopicKey | '' = selected
    ? `agent:${agentId}:${selected.type}:${selected.slug}`
    : ''
  const body = topicKey ? (topics as Record<TopicKey, { body?: string }>)[topicKey]?.body : ''

  return (
    <div className="flex h-full min-h-0">
      <aside className="w-1/3 min-w-[200px] overflow-y-auto border-r p-3">
        <div className="mb-3 flex flex-wrap gap-1.5">
          {(['all', ...TYPES] as const).map((t) => (
            <button
              key={t}
              onClick={() => setFilter(t)}
              className={`text-xs px-2 py-1 rounded ${
                filter === t
                  ? 'bg-primary text-primary-foreground'
                  : 'bg-muted text-muted-foreground hover:bg-accent'
              }`}
            >
              {t}
            </button>
          ))}
        </div>
        <ul className="space-y-1">
          {visible.length === 0 && (
            <li className="text-sm text-muted-foreground">
              {entries ? 'No entries match this filter.' : 'No memory yet.'}
            </li>
          )}
          {visible.map((e) => (
            <li key={e.fullName}>
              <button
                onClick={() => {
                  setSelected(e)
                  void loadTopic(agentId, e.type, e.slug)
                }}
                className={`block text-left w-full py-1 px-2 rounded hover:bg-accent text-sm ${
                  selected?.fullName === e.fullName ? 'bg-accent' : ''
                }`}
              >
                <span className="font-mono text-xs text-muted-foreground mr-2">{e.type}</span>
                {e.desc}
              </button>
            </li>
          ))}
        </ul>
      </aside>
      <section className="flex-1 min-w-0 overflow-y-auto p-4">
        {selected && body ? (
          <MarkdownRenderer>{body}</MarkdownRenderer>
        ) : (
          <p className="text-sm text-muted-foreground">Select a topic on the left.</p>
        )}
      </section>
    </div>
  )
}

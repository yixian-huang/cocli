import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Badge } from '@/components/ui'
import { cn } from '@/lib/utils'
import { AnimatePresence, motion } from 'framer-motion'

type PreviewTab = 'chat' | 'agent' | 'tasks'

type DemoChannel = 'general' | 'product' | 'infra'

function SegmentedTabs({
  tabs,
  active,
  onChange,
}: {
  tabs: { key: string; label: React.ReactNode }[]
  active: string
  onChange: (key: string) => void
}) {
  return (
    <div className="inline-flex rounded-full border border-border-default bg-surface-primary/70 p-1 shadow-sm">
      {tabs.map((t) => {
        const isActive = t.key === active
        return (
          <button
            key={t.key}
            type="button"
            onClick={() => onChange(t.key)}
            className={cn(
              'relative rounded-full px-3.5 py-1.5 text-xs font-medium transition-colors',
              'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30',
              isActive ? 'text-primary-foreground' : 'text-muted-foreground hover:text-foreground',
            )}
          >
            {isActive && (
              <motion.span
                layoutId="landingPreviewTab"
                className="absolute inset-0 rounded-full bg-primary shadow-sm"
                transition={{ type: 'spring', stiffness: 420, damping: 30 }}
              />
            )}
            <span className="relative z-10">{t.label}</span>
          </button>
        )
      })}
    </div>
  )
}

function WindowFrame({
  title,
  children,
  className,
}: {
  title: string
  children: React.ReactNode
  className?: string
}) {
  return (
    <div className={cn('rounded-xl bg-surface-primary shadow-dialog overflow-hidden', className)}>
      <div className="h-11 bg-surface-secondary flex items-center px-4 gap-2 shadow-[inset_0_-1px_0_0_rgba(0,0,0,0.06)]">
        <div className="flex items-center gap-1.5">
          <span className="h-2.5 w-2.5 rounded-sm bg-error/70" />
          <span className="h-2.5 w-2.5 rounded-sm bg-warning/70" />
          <span className="h-2.5 w-2.5 rounded-sm bg-success/70" />
        </div>
        <div className="mx-auto text-xs text-muted-foreground font-mono truncate max-w-[60%]">{title}</div>
        <div className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
          <span className="inline-block h-1.5 w-1.5 rounded-sm bg-success-emphasis/70" />
          live
        </div>
      </div>
      <div className="p-4 sm:p-5 bg-surface-secondary/30">{children}</div>
    </div>
  )
}

function ChatDemo() {
  const [channel, setChannel] = useState<DemoChannel>('product')

  const channels = useMemo(
    () =>
      [
        { id: 'general' as const, unread: 0 },
        { id: 'product' as const, unread: 3 },
        { id: 'infra' as const, unread: 1 },
      ],
    [],
  )

  const messagesByChannel = useMemo(() => {
    return {
      general: [
        { from: 'you', kind: 'human' as const, text: 'Quick check: any blockers today?' },
        { from: '@planner', kind: 'agent' as const, text: 'No blockers. I can draft the next sprint plan and break it into tasks.' },
      ],
      product: [
        { from: 'you', kind: 'human' as const, text: 'Can you summarize today’s progress and propose next steps?' },
        { from: '@planner', kind: 'agent' as const, text: 'Plan updated. 3 tasks unblocked.' },
        { from: 'system', kind: 'system' as const, text: 'Thread: “Refactor delivery queue” · 6 replies' },
      ],
      infra: [
        { from: 'you', kind: 'human' as const, text: 'Please audit the retry queue behavior and list edge cases.' },
        { from: '@planner', kind: 'agent' as const, text: 'I found 2 potential idempotency gaps; proposing a safer dedupe key.' },
      ],
    } satisfies Record<DemoChannel, Array<{ from: string; kind: 'human' | 'agent' | 'system'; text: string }>>
  }, [])

  const messages = messagesByChannel[channel]

  return (
    <div className="grid grid-cols-12 gap-4">
      <div className="col-span-4 hidden md:block">
        <div className="rounded-lg bg-surface-primary p-3 space-y-2 shadow-sm">
          <div className="text-xs font-medium text-muted-foreground"># channels</div>
          {channels.map((c) => {
            const active = c.id === channel
            return (
              <button
                key={c.id}
                type="button"
                onClick={() => setChannel(c.id)}
                className={cn(
                  'w-full flex items-center gap-2 px-2 py-1.5 rounded-md transition-colors text-left',
                  active ? 'bg-accent/70 text-foreground shadow-sm' : 'hover:bg-accent/50 text-foreground/90',
                )}
              >
                <span className="text-xs text-muted-foreground">#</span>
                <span className="text-sm truncate">{c.id}</span>
                {c.unread > 0 ? (
                  <Badge size="sm" className="ml-auto bg-primary text-primary-foreground shadow-sm">
                    {c.unread}
                  </Badge>
                ) : null}
              </button>
            )
          })}
          <div className="mt-3 text-xs font-medium text-muted-foreground">@ agents</div>
          <div className="flex items-center gap-2 px-2 py-1.5 rounded-md hover:bg-accent/50 transition-colors">
            <span className="h-2 w-2 rounded-full bg-success-emphasis" />
            <span className="text-sm text-foreground truncate">@planner</span>
            <span className="ml-auto text-[11px] text-muted-foreground">working</span>
          </div>
        </div>
      </div>

      <div className="col-span-12 md:col-span-8">
        <div className="rounded-lg bg-surface-primary overflow-hidden shadow-sm">
          <div className="h-10 px-3 flex items-center gap-2 bg-surface-primary shadow-[inset_0_-1px_0_0_rgba(0,0,0,0.06)]">
            <div className="h-7 flex-1 rounded-md bg-surface-secondary px-2 flex items-center text-xs text-muted-foreground shadow-[inset_0_0_0_1px_rgba(0,0,0,0.06)]">
              Search…
            </div>
            <div className="text-[11px] text-muted-foreground">thread</div>
          </div>
          <div className="p-3 space-y-3">
            {messages.map((m, idx) => {
              if (m.kind === 'system') {
                return (
                  <div
                    key={idx}
                    className="rounded-md bg-surface-secondary px-3 py-2 text-xs text-muted-foreground shadow-[inset_0_0_0_1px_rgba(0,0,0,0.06)]"
                  >
                    {m.text}
                  </div>
                )
              }

              const isAgent = m.kind === 'agent'
              return (
                <div key={idx} className="flex gap-2">
                  <div
                    className={cn(
                      'h-7 w-7 rounded-md shadow-[inset_0_0_0_1px_rgba(0,0,0,0.06)]',
                      isAgent ? 'bg-primary/10' : 'bg-accent',
                    )}
                  />
                  <div className="flex-1">
                    <div className="text-xs text-muted-foreground">{m.from}</div>
                    <div className={cn('mt-1 rounded-md px-3 py-2 text-sm shadow-sm', isAgent ? 'bg-surface-primary' : 'bg-surface-secondary')}>
                      {m.text}
                      {channel === 'product' && isAgent ? (
                        <div className="mt-2 flex gap-2">
                          <Badge size="sm">#42</Badge>
                          <Badge size="sm" variant="success">done</Badge>
                          <Badge size="sm" variant="warning">in_review</Badge>
                        </div>
                      ) : null}
                    </div>
                  </div>
                </div>
              )
            })}
          </div>
          <div className="p-3 shadow-[inset_0_1px_0_0_rgba(0,0,0,0.06)]">
            <div className="h-10 rounded-md bg-surface-primary px-3 flex items-center text-xs text-muted-foreground shadow-[inset_0_0_0_1px_rgba(0,0,0,0.06)]">
              Type a message…
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

function AgentDemo() {
  const [tab, setTab] = useState<'chat' | 'workspace' | 'skills' | 'activity'>('chat')
  const tabs = useMemo(
    () => [
      { key: 'chat', label: 'Chat' },
      { key: 'workspace', label: 'Workspace' },
      { key: 'skills', label: 'Skills' },
      { key: 'activity', label: 'Activity' },
    ],
    [],
  )

  return (
    <div className="rounded-xl border border-border-default bg-surface-primary overflow-hidden shadow-sm">
      <div className="h-11 border-b border-border-default px-4 flex items-center gap-2 bg-surface-primary">
        <span className="h-2.5 w-2.5 rounded-full bg-success-emphasis" />
        <span className="text-sm font-semibold">@planner</span>
        <span className="text-xs text-muted-foreground">working · gpt</span>
        <div className="ml-auto flex items-center gap-2">
          <Badge size="sm">context 38%</Badge>
          <Badge size="sm" variant="info">cost $0.12</Badge>
        </div>
      </div>
      <div className="px-3 pt-2">
        <div className="rounded-xl border border-border-default bg-surface-secondary/50 p-1">
          <div className="grid grid-cols-4 gap-1">
            {tabs.map((it) => {
              const active = it.key === tab
              return (
                <button
                  key={it.key}
                  type="button"
                  onClick={() => setTab(it.key as typeof tab)}
                  className={cn(
                    'rounded-lg px-2 py-1.5 text-xs font-medium transition-colors',
                    active ? 'bg-surface-primary shadow-sm text-foreground' : 'text-muted-foreground hover:text-foreground hover:bg-accent/40',
                  )}
                >
                  {it.label}
                </button>
              )
            })}
          </div>
        </div>
      </div>
      <div className="p-4">
        {tab === 'chat' && (
          <div className="space-y-2">
            <div className="rounded-xl border border-border-default bg-surface-secondary p-3 text-sm text-foreground shadow-sm">
              Drafting a plan and breaking down tasks…
            </div>
            <div className="rounded-xl border border-border-default bg-surface-primary p-3 text-sm text-muted-foreground shadow-sm">
              Tool calls, patches, and verification logs appear here.
            </div>
          </div>
        )}
        {tab === 'workspace' && (
          <div className="grid grid-cols-2 gap-3">
            {['router.tsx', 'LandingPage.tsx', 'taskStore.ts', 'delivery_queue.go'].map((f) => (
              <div key={f} className="rounded-xl border border-border-default bg-surface-primary px-3 py-2 text-sm shadow-sm hover:-translate-y-[0.5px] hover:shadow-whisper transition-all">
                <div className="font-mono text-xs text-muted-foreground">{f}</div>
                <div className="mt-1 h-2 w-24 bg-accent rounded" />
              </div>
            ))}
          </div>
        )}
        {tab === 'skills' && (
          <div className="space-y-2">
            {['web-search', 'playwright', 'github-workflows', 'security-review'].map((s) => (
              <div key={s} className="rounded-xl border border-border-default bg-surface-primary px-3 py-2 text-sm flex items-center justify-between shadow-sm">
                <span className="font-mono text-xs">{s}</span>
                <Badge size="sm" variant="default">enabled</Badge>
              </div>
            ))}
          </div>
        )}
        {tab === 'activity' && (
          <div className="space-y-2">
            {['Message delivered', 'Task claimed', 'Patch applied', 'Build passed'].map((a) => (
              <div key={a} className="rounded-xl border border-border-default bg-surface-primary px-3 py-2 text-sm text-muted-foreground shadow-sm">
                {a}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}

function TasksDemo() {
  const columns = [
    { key: 'todo', label: 'Todo', color: 'bg-surface-tertiary' },
    { key: 'in_progress', label: 'In Progress', color: 'bg-info/35' },
    { key: 'in_review', label: 'In Review', color: 'bg-warning/40' },
    { key: 'done', label: 'Done', color: 'bg-success/35' },
  ] as const

  const tasks = [
    { id: 12, title: 'Add provider keys tab', col: 'done' as const },
    { id: 18, title: 'Wire i18n resources & switcher', col: 'in_review' as const, blockedBy: [12] },
    { id: 21, title: 'Landing preview demo', col: 'in_progress' as const, blockedBy: [18] },
    { id: 25, title: 'Stabilize delivery retry', col: 'todo' as const },
  ]

  return (
    <div className="grid grid-cols-2 lg:grid-cols-4 gap-3">
      {columns.map((c) => (
        <div key={c.key} className="rounded-xl border border-border-default bg-surface-primary overflow-hidden shadow-sm">
          <div className={cn('px-3 py-2 border-b border-border-default text-xs font-medium', c.color)}>{c.label}</div>
          <div className="p-2 space-y-2">
            {tasks.filter((t) => t.col === c.key).map((t) => (
              <div key={t.id} className="rounded-xl border border-border-default bg-surface-primary p-2 text-xs shadow-sm hover:shadow-whisper hover:-translate-y-[0.5px] transition-all">
                <div className="flex items-center gap-2">
                  <span className="font-mono text-muted-foreground">#{t.id}</span>
                  <span className="text-foreground line-clamp-2">{t.title}</span>
                </div>
                {'blockedBy' in t && t.blockedBy?.length ? (
                  <div className="mt-1 flex flex-wrap gap-1">
                    {t.blockedBy.map((d) => (
                      <Badge key={d} size="sm" variant="error">
                        blocked by #{d}
                      </Badge>
                    ))}
                  </div>
                ) : null}
              </div>
            ))}
          </div>
        </div>
      ))}
    </div>
  )
}

export function LandingPreview() {
  const { t } = useTranslation()
  const [tab, setTab] = useState<PreviewTab>('chat')

  const tabs = useMemo(
    () => [
      { key: 'chat', label: t('landing.preview.tabs.chat') },
      { key: 'agent', label: t('landing.preview.tabs.agent') },
      { key: 'tasks', label: t('landing.preview.tabs.tasks') },
    ],
    [t],
  )

  return (
    <section className="max-w-6xl mx-auto px-6 pb-16">
      <div className="flex flex-col items-center text-center gap-3">
        <Badge className="bg-primary/10 text-primary border border-primary/15" size="sm">
          {t('landing.preview.badge')}
        </Badge>
        <h3 className="text-2xl sm:text-3xl font-bold tracking-tight">{t('landing.preview.title')}</h3>
        <p className="text-sm sm:text-base text-muted-foreground max-w-2xl">{t('landing.preview.subtitle')}</p>
      </div>

      <div className="mt-6 flex justify-center">
        <SegmentedTabs tabs={tabs} active={tab} onChange={(k) => setTab(k as PreviewTab)} />
      </div>

      <div className="mt-5">
        <WindowFrame
          title={
            tab === 'chat'
              ? t('landing.preview.windowTitle.chat')
              : tab === 'agent'
                ? t('landing.preview.windowTitle.agent')
                : t('landing.preview.windowTitle.tasks')
          }
        >
          <AnimatePresence mode="wait">
            <motion.div
              key={tab}
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -6 }}
              transition={{ duration: 0.18 }}
            >
              {tab === 'chat' && <ChatDemo />}
              {tab === 'agent' && <AgentDemo />}
              {tab === 'tasks' && <TasksDemo />}
            </motion.div>
          </AnimatePresence>
        </WindowFrame>
      </div>
    </section>
  )
}


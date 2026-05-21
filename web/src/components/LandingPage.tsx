import { Bot, MessageSquare, ListTodo, Cpu, Zap, GitBranch, ArrowRight } from 'lucide-react'
import { useCallback } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { LanguageSwitcher } from '@/components/ui'
import { LandingPreview } from '@/components/landing/LandingPreview'
import { BrandLogo } from '@/components/BrandLogo'
import { Button } from '@/components/ui'
import { useUserStore } from '@/stores/userStore'
import { BRAND } from '@/brand'

export function LandingPage() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const location = useLocation()
  const user = useUserStore((s) => s.user)

  const handleEnter = useCallback(() => {
    if (user) {
      navigate('/')
      return
    }
    navigate('/login', { state: { from: location.pathname + location.search } })
  }, [user, navigate, location.pathname, location.search])

  const features = [
    { icon: Bot, title: t('landing.features.agentManagement.title'), desc: t('landing.features.agentManagement.desc') },
    { icon: MessageSquare, title: t('landing.features.realtimeCollab.title'), desc: t('landing.features.realtimeCollab.desc') },
    { icon: ListTodo, title: t('landing.features.taskOrchestration.title'), desc: t('landing.features.taskOrchestration.desc') },
    { icon: Zap, title: t('landing.features.reliableDelivery.title'), desc: t('landing.features.reliableDelivery.desc') },
    { icon: Cpu, title: t('landing.features.distributedArch.title'), desc: t('landing.features.distributedArch.desc') },
    { icon: GitBranch, title: t('landing.features.taskDependencies.title'), desc: t('landing.features.taskDependencies.desc') },
  ]

  const architecture = [
    { name: 'Web', tech: 'React + TypeScript', desc: t('landing.arch.webDesc') },
    { name: 'Server', tech: 'Go + Gin + PostgreSQL', desc: t('landing.arch.serverDesc') },
    { name: 'Daemon', tech: 'Go', desc: t('landing.arch.daemonDesc') },
    { name: 'Bridge', tech: 'Go MCP Server', desc: t('landing.arch.bridgeDesc') },
  ]

  const howItWorksSteps = [
    { step: '1', title: t('landing.how.steps.1.title'), desc: t('landing.how.steps.1.desc') },
    { step: '2', title: t('landing.how.steps.2.title'), desc: t('landing.how.steps.2.desc') },
    { step: '3', title: t('landing.how.steps.3.title'), desc: t('landing.how.steps.3.desc') },
    { step: '4', title: t('landing.how.steps.4.title'), desc: t('landing.how.steps.4.desc') },
  ]

  return (
    <div className="min-h-screen bg-background text-foreground">
      {/* Hero */}
      <header className="border-b bg-linear-to-b from-background to-background/60">
        <div className="max-w-6xl mx-auto px-6 py-4 flex items-center justify-between">
          <BrandLogo iconClassName="h-8 w-8" textClassName="text-xl" />
          <div className="flex items-center gap-3">
            <LanguageSwitcher compact />
            <Button size="md" variant="primary" onClick={handleEnter} className="shadow-sm hover:shadow-md hover:-translate-y-[0.5px] transition-all">
              {user ? t('landing.openWorkspace') : t('landing.getStarted')}
            </Button>
          </div>
        </div>
      </header>

      <main>
        {/* Hero section */}
        <section className="relative bg-surface-canvas">
          <div
            className="pointer-events-none absolute inset-x-0 top-0 h-[30vh]"
            style={{
              background:
                'linear-gradient(180deg, color-mix(in srgb, var(--accent-signature) 4%, transparent) 0%, transparent 100%)',
            }}
            aria-hidden
          />
          <div className="relative z-10 max-w-6xl mx-auto px-6 pt-16 pb-12 sm:pt-20 sm:pb-16 text-center">
          <h2 className="mx-auto max-w-4xl text-4xl font-bold tracking-tight leading-tight sm:text-6xl">
            {t('landing.heroTitle')}
          </h2>
          <p className="mt-5 text-base sm:text-lg text-muted-foreground max-w-2xl mx-auto leading-relaxed">
            {t('landing.heroSubtitle')}
          </p>
          <div className="mt-8 flex flex-col sm:flex-row justify-center items-center gap-3">
            <Button
              size="lg"
              variant="primary"
              onClick={handleEnter}
              className="shadow-sm hover:shadow-md hover:-translate-y-[0.5px] transition-all"
            >
              {t('landing.openWorkspace')} <ArrowRight className="h-4 w-4" />
            </Button>
            <a
              href={BRAND.githubUrl}
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex items-center gap-2 rounded-lg border border-border-default bg-surface-primary/60 px-6 py-3 text-sm font-medium hover:bg-accent transition-colors shadow-sm"
            >
              <GitBranch className="h-4 w-4" />
              {t('landing.viewSource')}
            </a>
          </div>
          </div>
        </section>

        <LandingPreview />

        {/* Features */}
        <section className="border-t bg-accent/30">
          <div className="max-w-6xl mx-auto px-6 py-14 sm:py-16">
            <h3 className="text-2xl sm:text-3xl font-bold text-center mb-10 sm:mb-12">{t('landing.coreFeatures')}</h3>
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
              {features.map((f) => (
                <div
                  key={f.title}
                  className="group rounded-2xl border border-border-default bg-surface-primary p-6 space-y-3 shadow-sm hover:shadow-whisper transition-all hover:-translate-y-px"
                >
                  <div className="h-10 w-10 rounded-xl bg-primary/10 flex items-center justify-center shadow-ring group-hover:bg-primary/12 transition-colors">
                    <f.icon className="h-5 w-5 text-primary" />
                  </div>
                  <h4 className="font-semibold">{f.title}</h4>
                  <p className="text-sm text-muted-foreground leading-relaxed">{f.desc}</p>
                </div>
              ))}
            </div>
          </div>
        </section>

        {/* Architecture */}
        <section className="border-t">
          <div className="max-w-6xl mx-auto px-6 py-14 sm:py-16">
            <h3 className="text-2xl sm:text-3xl font-bold text-center mb-10 sm:mb-12">{t('landing.architecture')}</h3>
            <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
              {architecture.map((a) => (
                <div
                  key={a.name}
                  className="group rounded-2xl border border-border-default bg-surface-primary p-5 space-y-2 shadow-sm hover:shadow-whisper transition-all hover:-translate-y-px"
                >
                  <div className="text-lg font-bold">{a.name}</div>
                  <div className="text-xs font-mono text-primary">{a.tech}</div>
                  <p className="text-sm text-muted-foreground">{a.desc}</p>
                </div>
              ))}
            </div>
            <div className="mt-8 text-center text-sm text-muted-foreground">
              <code className="bg-accent px-2 py-1 rounded text-xs">
                Web &harr; Server &harr; Daemon &harr; Agent (Claude CLI)
              </code>
            </div>
          </div>
        </section>

        {/* How it works */}
        <section className="border-t bg-accent/30">
          <div className="max-w-5xl mx-auto px-6 py-16">
            <h3 className="text-2xl font-bold text-center mb-12">{t('landing.howItWorks')}</h3>
            <div className="max-w-2xl mx-auto space-y-6">
              {howItWorksSteps.map((s) => (
                <div key={s.step} className="flex gap-4 items-start">
                  <div className="h-8 w-8 rounded-full bg-primary text-primary-foreground flex items-center justify-center text-sm font-bold shrink-0">
                    {s.step}
                  </div>
                  <div>
                    <div className="font-semibold">{s.title}</div>
                    <p className="text-sm text-muted-foreground">{s.desc}</p>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </section>

        {/* CTA */}
        <section className="border-t">
          <div className="max-w-5xl mx-auto px-6 py-16 text-center">
            <h3 className="text-2xl font-bold">{t('landing.readyToGetStarted')}</h3>
            <p className="mt-2 text-muted-foreground">{t('landing.ctaSubtitle')}</p>
            <button
              onClick={handleEnter}
              className="mt-6 inline-flex items-center gap-2 rounded-lg bg-primary text-primary-foreground px-6 py-3 text-sm font-medium hover:bg-primary/90 transition-colors"
            >
              {t('landing.openWorkspace')} <ArrowRight className="h-4 w-4" />
            </button>
          </div>
        </section>
      </main>

      {/* Footer */}
      <footer className="border-t py-6">
        <div className="max-w-5xl mx-auto px-6 flex items-center justify-between text-xs text-muted-foreground">
          <span>{t('landing.footerTagline')}</span>
          <a
            href={BRAND.githubUrl}
            target="_blank"
            rel="noopener noreferrer"
            className="hover:text-foreground transition-colors"
          >
            GitHub
          </a>
        </div>
      </footer>
    </div>
  )
}

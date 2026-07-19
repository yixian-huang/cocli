import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { LocalApp } from './LocalApp'

const runtime = {
  name: 'fake',
  installed: true,
  binary: null,
  version: 'local-loop',
  models: ['test-model'],
  capabilities: ['reply'],
  unavailable_reason: null,
}

const channel = {
  id: 'channel-1',
  name: 'product-loop',
  description: null,
  goal: null,
  kind: 'standard',
  is_system: false,
  direct_agent_id: null,
  created_by_agent_id: null,
  created_by_channel_id: null,
  created_at: '2026-07-16T09:00:00Z',
}

const createdAgent = {
  id: 'agent-1',
  name: 'builder',
  description: null,
  instructions: null,
  runtime: 'fake',
  model: 'test-model',
  status: 'running',
  lifecycle_status: 'active',
  created_by_agent_id: null,
  created_by_channel_id: null,
  created_at: '2026-07-16T09:01:00Z',
}

function jsonResponse(body: unknown, status = 200) {
  return Promise.resolve(new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  }))
}

describe('LocalApp', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  beforeEach(() => {
    let skillInstalled = false
    let governanceProfileCreated = false
    let governanceBound = false
    let agentCreated = false
    let taskState: Array<{
      id: string
      channelId: string
      taskNumber: number
      title: string
      status: 'todo' | 'in_progress' | 'in_review' | 'done'
      progress?: string
      assigneeId?: string
      assigneeType?: string
      assigneeName?: string
      createdAt: string
      updatedAt: string
    }> = []
    const taskDependencies: Record<number, number[]> = {}
    let memoryTopic: {
      type: 'project'
      topic: string
      description: string
      updated: string
      body: string
      path: string
      version: number
    } | null = null
    localStorage.clear()
    window.matchMedia = vi.fn().mockReturnValue({
      matches: false,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    })
    Element.prototype.scrollIntoView = vi.fn()
    vi.stubGlobal('fetch', vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
      const path = String(input)
      if (path === '/api/runtimes') return jsonResponse([runtime])
      if (path === '/api/search?q=needle') {
        return jsonResponse({
          results: [{
            kind: 'message',
            id: 'message-search',
            title: '#product-loop · message #8',
            snippet: 'needle in local history',
            channelId: channel.id,
            agentId: null,
            messageId: 'message-search',
            taskNumber: null,
            path: null,
          }],
        })
      }
      if (path === '/api/runtimes/compatibility') {
        return jsonResponse({ fake: 'supported' })
      }
      if (path.startsWith('/api/runtimes/skills/doctor')) {
        const evidence = {
          source: 'filesystem',
          detail: 'runtime driver search paths',
          provesSessionVisibility: false,
        }
        return jsonResponse({
          observedAt: '2026-07-19T08:00:00Z',
          cacheStatus: path.includes('force=true') ? 'fresh' : 'cached',
          forceRefresh: path.includes('force=true'),
          summary: {
            status: 'ok',
            runtimeCount: 1,
            agentCount: agentCreated ? 1 : 0,
            skillCount: skillInstalled ? 2 : 0,
            issueCount: 0,
            errorCount: 0,
            warningCount: 0,
          },
          runtimes: [{
            runtime: 'fake',
            compatibility: 'supported',
            agentCount: agentCreated ? 1 : 0,
            skillCount: skillInstalled ? 2 : 0,
            issueCount: 0,
            evidenceSources: agentCreated ? ['filesystem'] : [],
            observedAt: '2026-07-19T08:00:00Z',
            cacheStatus: 'cached',
            expiresAt: '2026-07-19T08:00:03Z',
            evidence,
            searchPaths: [],
            skills: [],
            issues: [],
          }],
          agents: agentCreated ? [{
            observedAt: '2026-07-19T08:00:00Z',
            cacheStatus: 'cached',
            expiresAt: '2026-07-19T08:00:03Z',
            agentId: 'agent-1',
            agentName: 'builder',
            runtime: 'fake',
            compatibility: 'supported',
            evidence,
            searchPaths: [{
              path: '.fake/skills',
              scope: 'workspace',
              exists: true,
              readable: true,
              symlink: false,
            }],
            skills: [],
            issues: [],
          }] : [],
          diagnostics: [],
        })
      }
      if (path === '/api/skills/governance/profiles' && !init?.method) {
        return jsonResponse(governanceProfileCreated ? [{
          id: 'profile-1',
          version: 1,
          schemaVersion: 1,
          name: 'default-governance-profile',
          description: 'Safe default profile with no desired skills yet.',
          skills: [],
          createdAt: '2026-07-19T08:00:00Z',
          updatedAt: '2026-07-19T08:00:00Z',
        }] : [])
      }
      if (path === '/api/skills/governance/profiles' && init?.method === 'POST') {
        governanceProfileCreated = true
        const body = JSON.parse(String(init.body)) as {
          schemaVersion: number
          name: string
          description: string
          skills: unknown[]
        }
        return jsonResponse({
          id: 'profile-1',
          version: 1,
          ...body,
          createdAt: '2026-07-19T08:00:00Z',
          updatedAt: '2026-07-19T08:00:00Z',
        }, 201)
      }
      if (path === '/api/skills/governance/bindings' && !init?.method) {
        return jsonResponse(governanceBound ? [{
          id: 'binding-1',
          scope: 'machine',
          scopeId: 'machine',
          profileId: 'profile-1',
          version: 1,
          createdAt: '2026-07-19T08:01:00Z',
          updatedAt: '2026-07-19T08:01:00Z',
        }] : [])
      }
      if (path === '/api/skills/governance/bindings' && init?.method === 'POST') {
        governanceBound = true
        const body = JSON.parse(String(init.body)) as {
          profileId: string
          scope: 'machine' | 'workspace' | 'agent'
          scopeId: string
        }
        return jsonResponse({
          id: 'binding-1',
          ...body,
          version: 1,
          createdAt: '2026-07-19T08:01:00Z',
          updatedAt: '2026-07-19T08:01:00Z',
        }, 201)
      }
      if (path.startsWith('/api/skills/governance/desired/effective')) {
        return jsonResponse({
          schemaVersion: 1,
          desiredConfigHash: 'desired-hash-1',
          skills: [],
          conflicts: [],
        })
      }
      if (path.startsWith('/api/skills/governance/evidence')) {
        return jsonResponse({
          observedAt: '2026-07-19T08:02:00Z',
          snapshotHash: 'observation-hash-1',
          skills: [{
            logicalIdentity: 'reviewer',
            runtime: 'fake',
            scope: 'workspace',
            sourceProvenance: 'local:/tmp/reviewer',
            version: null,
            contentDigest: 'sha256:content',
            manifestDigest: 'sha256:manifest',
            installationMode: 'copy',
            destination: '.fake/skills/reviewer',
            fingerprint: 'governance-skill-1',
            enabled: true,
            shadowed: false,
            brokenSymlink: false,
            evidenceStatus: 'observed',
            evidenceSource: 'filesystem',
            sessionEffective: 'unknown',
            sessionReason: 'running session visibility cannot be proven',
            observedAt: '2026-07-19T08:02:00Z',
            supported: true,
          }],
          diagnostics: [],
        })
      }
      if (path === '/api/skills/governance/lock/preview' && init?.method === 'POST') {
        return jsonResponse({
          snapshot: {
            id: 'lock-1',
            scope: 'machine',
            scopeId: 'machine',
            profileId: null,
            snapshot: {},
            observationHash: 'observation-hash-1',
            desiredHash: 'desired-hash-1',
            lockHash: 'lock-hash-1',
            createdAt: '2026-07-19T08:03:00Z',
          },
          preview: {
            observedAt: '2026-07-19T08:03:00Z',
            snapshotHash: 'observation-hash-1',
            desiredConfigHash: 'desired-hash-1',
            lockfileHash: 'lock-hash-1',
            content: {
              schemaVersion: 1,
              generatedFrom: {
                observationHash: 'observation-hash-1',
                desiredConfigHash: 'desired-hash-1',
              },
              entries: [],
            },
            serialized: '{}\n',
          },
          drift: [{
            fingerprint: 'drift-1',
            skillFingerprint: 'governance-skill-1',
            kind: 'unknown_evidence',
            logicalIdentity: 'reviewer',
            runtime: 'fake',
            scope: 'workspace',
            reason: 'session-effective unknown',
            expected: 'desired',
            actual: 'observed',
          }],
          previousLockHash: null,
          lockfileChanged: true,
          writesRealDirectories: false,
          lockfileBoundary: 'store_only',
        })
      }
      if (path === '/api/skills/governance/plans' && init?.method === 'POST') {
        return jsonResponse({
          plan: {
            id: 'plan-1',
            scope: 'machine',
            scopeId: 'machine',
            plan: {
              schemaVersion: 1,
              dryRun: true,
              applied: false,
              lockfileChanged: true,
              preview: {
                planHash: 'plan-hash-1',
                dryRun: true,
                content: {
                  schemaVersion: 1,
                  observationHash: 'observation-hash-1',
                  desiredConfigHash: 'desired-hash-1',
                  lockfileHash: 'lock-hash-1',
                  actions: [],
                },
              },
            },
            observationHash: 'observation-hash-1',
            desiredHash: 'desired-hash-1',
            status: 'approved',
            version: 1,
            createdAt: '2026-07-19T08:04:00Z',
            updatedAt: '2026-07-19T08:04:00Z',
          },
          preview: {
            planHash: 'plan-hash-1',
            dryRun: true,
            content: {
              schemaVersion: 1,
              observationHash: 'observation-hash-1',
              desiredConfigHash: 'desired-hash-1',
              lockfileHash: 'lock-hash-1',
              actions: [{
                action: 'manual',
                runtime: 'fake',
                scope: 'workspace',
                target: 'reviewer',
                skillFingerprint: 'governance-skill-1',
                before: 'unknown',
                after: 'review',
                risk: 'medium',
                reason: 'session-effective unknown',
                evidence: 'filesystem',
                expectedObservationHash: 'observation-hash-1',
                expectedConfigHash: 'desired-hash-1',
                expectedLockHash: 'lock-hash-1',
                approvalRequired: true,
                blocked: false,
              }],
            },
          },
          drift: [],
          lockSnapshotId: 'lock-1',
          lockfileChanged: true,
          applied: false,
        }, 201)
      }
      if (path === '/api/skills/governance/plans?scope=machine&scopeId=machine' && !init?.method) {
        return jsonResponse([])
      }
      if (path === '/api/skills/governance/locks?scope=machine&scopeId=machine' && !init?.method) {
        return jsonResponse([])
      }
      if (path === '/api/channels' && !init?.method) return jsonResponse([channel])
      if (path === '/api/agents' && !init?.method) {
        return jsonResponse(agentCreated ? [createdAgent] : [])
      }
      if (path === '/api/zones/local/skills/library' && !init?.method) {
        return jsonResponse({
          entries: [{
            id: 'library-1',
            zoneId: 'local',
            name: 'reviewer',
            displayName: 'Reviewer',
            description: 'Review local changes',
            userInvocable: true,
            sourceKind: 'local',
            sourceUrl: '/tmp/reviewer',
            totalBytes: 128,
            fileCount: 1,
            importedBy: 'local',
            importedAt: '2026-07-16T09:00:00Z',
            updatedAt: '2026-07-16T09:00:00Z',
            inUseCount: skillInstalled ? 1 : 0,
          }],
        })
      }
      if (path === `/api/channels/${channel.id}/messages` && !init?.method) return jsonResponse([])
      if (path === `/api/channels/${channel.id}/agents` && !init?.method) {
        return jsonResponse(agentCreated ? [createdAgent] : [])
      }
      if (path === `/api/channels/${channel.id}/workspaces` && !init?.method) {
        return jsonResponse([])
      }
      if (path === '/api/agents' && init?.method === 'POST') {
        agentCreated = true
        return jsonResponse(createdAgent, 201)
      }
      if (path === '/api/agents/agent-1/messages' && !init?.method) {
        return jsonResponse([])
      }
      if (path === '/api/agents/agent-1/messages' && init?.method === 'POST') {
        const body = JSON.parse(String(init.body)) as { content: string }
        return jsonResponse({
          message: {
            id: 'direct-message-1',
            seq: 1,
            agent_id: null,
            role: 'user',
            content: body.content,
            created_at: '2026-07-16T09:06:00Z',
          },
          replies: [{
            id: 'direct-message-2',
            seq: 2,
            agent_id: 'agent-1',
            role: 'assistant',
            content: `direct: ${body.content}`,
            created_at: '2026-07-16T09:06:01Z',
          }],
        }, 201)
      }
      if (path === '/api/agents/agent-1/channels' && !init?.method) {
        return jsonResponse([channel])
      }
      if (path === '/api/agents/agent-1/workspaces' && !init?.method) {
        return jsonResponse([])
      }
      if (path === '/api/agents/agent-1/workspaces' && init?.method === 'POST') {
        const body = JSON.parse(String(init.body)) as { kind: string; locator: string }
        return jsonResponse({
          id: 'workspace-1',
          provider_key: body.kind,
          descriptor_version: 1,
          display_name: `${body.kind} workspace`,
          portable_locator: body.kind === 'external' ? body.locator : null,
          owner_type: 'agent',
          owner_id: 'agent-1',
          kind: body.kind,
          locator: body.locator,
          metadata: {},
          created_at: '2026-07-16T09:07:00Z',
          updated_at: '2026-07-16T09:07:00Z',
        }, 201)
      }
      if (path === '/api/agents/agent-1/working' && !init?.method) {
        return jsonResponse(null)
      }
      if (path === '/api/agents/agent-1/operations' && !init?.method) {
        return jsonResponse([])
      }
      if (path === '/api/agents/agent-1/stop' && init?.method === 'POST') {
        return jsonResponse({
          ...createdAgent,
          status: 'stopped',
          lifecycle_status: 'paused',
        })
      }
      if (path === '/api/agents/agent-1/skills' && !init?.method) {
        return jsonResponse({
          skills: skillInstalled ? [{
            name: 'reviewer',
            displayName: 'Reviewer',
            description: 'Review local changes',
            userInvocable: true,
            type: 'workspace',
            path: '.fake/skills/reviewer/SKILL.md',
            installPath: '.fake/skills/reviewer',
            state: 'managed',
            presence: 'installed',
            runtime: 'fake',
            scope: 'workspace',
            sourcePath: '.fake/skills/reviewer',
            evidence: {
              source: 'codex_app_server',
              detail: 'skills/list(forceReload)',
              provesSessionVisibility: false,
            },
            enabled: false,
            valid: true,
            duplicate: false,
            shadowed: false,
            issues: [],
            installId: 'install-1',
            libraryId: 'library-1',
            sourceUrl: '/tmp/reviewer',
          }, {
            name: 'shell-helper',
            displayName: 'Shell helper',
            description: 'External runtime-native helper',
            userInvocable: false,
            type: 'global',
            path: '~/.fake/skills/shell-helper/SKILL.md',
            installPath: '~/.fake/skills/shell-helper',
            state: 'external',
            presence: 'discovered',
            runtime: 'fake',
            scope: 'user',
            sourcePath: '~/.fake/skills/shell-helper',
            evidence: {
              source: 'filesystem',
              detail: 'runtime driver search paths',
              provesSessionVisibility: false,
            },
            valid: true,
            duplicate: false,
            shadowed: false,
            issues: [],
            installId: null,
            libraryId: null,
            sourceUrl: null,
          }] : [],
        })
      }
      if (path === '/api/agents/agent-1/skills' && init?.method === 'POST') {
        skillInstalled = true
        return jsonResponse({
          installId: 'install-1',
          installPath: '.fake/skills/reviewer',
          bytes: 128,
        })
      }
      if (path === '/api/agents/agent-1/skills/install-1/files' && !init?.method) {
        return jsonResponse({
          installPath: '.fake/skills/reviewer',
          files: [{ name: 'SKILL.md', isDir: false, size: 64 }],
        })
      }
      if (path === '/api/agents/agent-1/skills/install-1/files/SKILL.md' && !init?.method) {
        return jsonResponse({
          content: '# Reviewer\n\nReview local changes.',
          binary: false,
        })
      }
      if (path === '/api/agents/agent-1/sessions?limit=50' && !init?.method) {
        return jsonResponse([{
          id: 'session-row-1',
          agentId: 'agent-1',
          sessionId: 'session-1',
          channelId: channel.id,
          turnCount: 1,
          inputTokens: 12,
          outputTokens: 6,
          costUsd: 0.001,
          contextWindow: 100000,
          sessionType: 'chat',
          startedAt: '2026-07-16T09:02:00Z',
          endedAt: '2026-07-16T09:02:01Z',
          endReason: 'idle',
        }])
      }
      if (path === '/api/agents/agent-1/sessions/current' && !init?.method) {
        return jsonResponse(null)
      }
      if (path === '/api/agents/agent-1/activity?limit=100&offset=0' && !init?.method) {
        return jsonResponse([{
          id: 'activity-1',
          agentId: 'agent-1',
          activity: 'working',
          detail: 'Recording local history',
          trajectory: ['received', 'replied'],
          createdAt: '2026-07-16T09:02:00Z',
          sessionRowId: 'session-row-1',
          sessionId: 'session-1',
        }])
      }
      if (
        path === '/api/agents/agent-1/turns?limit=120&offset=0&sessionId=session-1'
        && !init?.method
      ) {
        return jsonResponse([{
          id: 'turn-1',
          agentId: 'agent-1',
          sessionId: 'session-1',
          turnNumber: 1,
          startedAt: '2026-07-16T09:02:00Z',
          endedAt: '2026-07-16T09:02:01Z',
          inputTokens: 12,
          outputTokens: 6,
          costUsd: 0.001,
          contextWindow: 100000,
          sessionType: 'chat',
          durationMs: 1000,
          entries: [{
            kind: 'text',
            text: 'Recorded locally.',
          }],
          messageRef: {
            channelId: channel.id,
            messageId: 'message-1',
            seq: 1,
            createdAt: '2026-07-16T09:02:00Z',
          },
        }])
      }
      if (path === '/api/bridge/agents/agent-1/memory/list?scope=agent' && !init?.method) {
        return jsonResponse({
          entries: memoryTopic ? [{
            path: memoryTopic.path,
            body: memoryTopic.body,
            version: memoryTopic.version,
          }] : [],
        })
      }
      if (
        path.startsWith('/api/bridge/agents/agent-1/memory/topic?scope=agent')
        && !init?.method
      ) {
        return memoryTopic
          ? jsonResponse(memoryTopic)
          : jsonResponse({ error: 'memory topic not found' }, 404)
      }
      if (path === '/api/bridge/agents/agent-1/memory/topic' && init?.method === 'POST') {
        const body = JSON.parse(String(init.body)) as {
          type: 'project'
          topic: string
          description: string
          body: string
          ifVersion?: number
        }
        memoryTopic = {
          type: body.type,
          topic: body.topic,
          description: body.description,
          updated: '2026-07-16',
          body: `---\ndescription: ${body.description}\ntype: ${body.type}\nupdated: 2026-07-16\n---\n\n${body.body}`,
          path: `agents/agent-1/memory/${body.type}_${body.topic}.md`,
          version: body.ifVersion ? body.ifVersion + 1 : 1,
        }
        return jsonResponse(memoryTopic)
      }
      if (path === `/api/channels/${channel.id}/tasks` && !init?.method) {
        return jsonResponse(taskState)
      }
      if (path === `/api/channels/${channel.id}/tasks` && init?.method === 'POST') {
        const body = JSON.parse(String(init.body)) as { title: string }
        const taskNumber = taskState.length + 1
        const task = {
          id: `task-${taskNumber}`,
          channelId: channel.id,
          taskNumber,
          title: body.title,
          status: 'todo' as const,
          createdAt: '2026-07-16T09:03:00Z',
          updatedAt: '2026-07-16T09:03:00Z',
        }
        taskState = [...taskState, task]
        taskDependencies[taskNumber] = []
        return jsonResponse(task, 201)
      }
      const dependencyMatch = path.match(
        new RegExp(`^/api/channels/${channel.id}/tasks/(\\d+)/dependencies$`),
      )
      if (dependencyMatch && !init?.method) {
        const taskNumber = Number(dependencyMatch[1])
        return jsonResponse({
          taskNumber,
          dependsOn: taskDependencies[taskNumber] ?? [],
        })
      }
      if (dependencyMatch && init?.method === 'POST') {
        const taskNumber = Number(dependencyMatch[1])
        const body = JSON.parse(String(init.body)) as { dependsOn: number }
        taskDependencies[taskNumber] = [
          ...(taskDependencies[taskNumber] ?? []),
          body.dependsOn,
        ]
        return jsonResponse({
          taskNumber,
          dependsOn: taskDependencies[taskNumber],
        }, 201)
      }
      const claimMatch = path.match(
        new RegExp(`^/api/channels/${channel.id}/tasks/(\\d+)/claim$`),
      )
      if (claimMatch && init?.method === 'POST') {
        const taskNumber = Number(claimMatch[1])
        const task = taskState.find((candidate) => candidate.taskNumber === taskNumber)!
        const updated = {
          ...task,
          status: 'in_progress' as const,
          assigneeId: 'agent-1',
          assigneeType: 'agent',
          assigneeName: 'builder',
          updatedAt: '2026-07-16T09:04:00Z',
        }
        taskState = taskState.map((candidate) => candidate.id === updated.id ? updated : candidate)
        return jsonResponse(updated)
      }
      const statusMatch = path.match(
        new RegExp(`^/api/channels/${channel.id}/tasks/(\\d+)/status$`),
      )
      if (statusMatch && init?.method === 'POST') {
        const taskNumber = Number(statusMatch[1])
        const body = JSON.parse(String(init.body)) as {
          status: 'todo' | 'in_progress' | 'in_review' | 'done'
          progress?: string
        }
        const task = taskState.find((candidate) => candidate.taskNumber === taskNumber)!
        const updated = {
          ...task,
          status: body.status,
          progress: body.progress,
          updatedAt: '2026-07-16T09:05:00Z',
        }
        taskState = taskState.map((candidate) => candidate.id === updated.id ? updated : candidate)
        return jsonResponse(updated)
      }
      if (path === `/api/channels/${channel.id}/messages` && init?.method === 'POST') {
        return jsonResponse({
          message: {
            id: 'message-1',
            channel_id: channel.id,
            seq: 1,
            agent_id: null,
            role: 'user',
            content: 'Ship the loop',
            created_at: '2026-07-16T09:02:00Z',
          },
          replies: [{
            id: 'message-2',
            channel_id: channel.id,
            seq: 2,
            agent_id: 'agent-1',
            role: 'assistant',
            content: 'echo: Ship the loop',
            created_at: '2026-07-16T09:02:01Z',
          }],
        }, 201)
      }
      return jsonResponse({ error: `Unhandled ${path}` }, 500)
    }))
  })

  it('creates an agent and runs a task through the local API', async () => {
    render(<LocalApp />)

    expect(await screen.findByRole('heading', { name: '# product-loop' })).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Set up your first local task' })).toBeInTheDocument()
    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'builder' } })
    fireEvent.click(screen.getByRole('button', { name: 'Add running agent' }))

    expect(await screen.findByText('builder')).toBeInTheDocument()
    fireEvent.change(screen.getByLabelText('Task for #product-loop'), {
      target: { value: 'Ship the loop' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Run task' }))

    expect(await screen.findByText('echo: Ship the loop')).toBeInTheDocument()
    await waitFor(() => expect(screen.getByText('Ship the loop')).toBeInTheDocument())
    expect(screen.queryByRole('heading', { name: 'Set up your first local task' })).not.toBeInTheDocument()
  })

  it('renders live runtime events from the execution stream', async () => {
    class FakeEventSource {
      static instance: FakeEventSource | null = null
      onopen: (() => void) | null = null
      onerror: (() => void) | null = null
      onmessage: ((event: MessageEvent<string>) => void) | null = null
      readonly url: string

      constructor(url: string) {
        this.url = url
        FakeEventSource.instance = this
      }

      close() {}
    }
    vi.stubGlobal('EventSource', FakeEventSource)
    render(<LocalApp />)

    expect(await screen.findByRole('heading', { name: '# product-loop' })).toBeInTheDocument()
    await waitFor(() => expect(FakeEventSource.instance?.url).toBe('/api/events'))
    const eventSource = FakeEventSource.instance
    expect(eventSource?.url).toBe('/api/events')
    eventSource?.onopen?.()
    eventSource?.onmessage?.(new MessageEvent('message', {
      data: JSON.stringify({
        kind: 'text_delta',
        channelId: channel.id,
        agentId: 'agent-live',
        messageId: 'message-live',
        payload: { text: 'Streaming now' },
        occurredAt: '2026-07-16T09:02:00Z',
      }),
    }))

    expect(await screen.findByText('Streaming now')).toBeInTheDocument()
    expect(screen.getByText('Live execution connected')).toBeInTheDocument()
  })

  it('uses a persistent Agent as a first-class task surface', async () => {
    render(<LocalApp />)

    expect(await screen.findByRole('heading', { name: '# product-loop' })).toBeInTheDocument()
    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'builder' } })
    fireEvent.click(screen.getByRole('button', { name: 'Add running agent' }))
    expect(await screen.findByText('builder')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Agents' }))
    expect(await screen.findByRole('heading', { name: '@builder' })).toBeInTheDocument()
    fireEvent.change(screen.getByLabelText('Workspace location'), {
      target: { value: '/tmp/general-purpose-work' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Attach Workspace' }))
    expect(await screen.findByText('directory workspace · directory')).toBeInTheDocument()

    fireEvent.change(screen.getByPlaceholderText('Describe the requirement for this Agent…'), {
      target: { value: 'Research a non-code topic' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Run task' }))
    expect(await screen.findByText('direct: Research a non-code topic')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Pause Agent' }))
    expect(await screen.findByText('paused')).toBeInTheDocument()
  })

  it('searches durable local data and exposes a state backup download', async () => {
    render(<LocalApp />)

    expect(await screen.findByRole('heading', { name: '# product-loop' })).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Download application-state backup' }))
      .toHaveAttribute('href', '/api/backups/state')
    fireEvent.click(screen.getByRole('button', { name: 'Global search' }))
    fireEvent.change(screen.getByRole('textbox', { name: 'Global search' }), {
      target: { value: 'needle' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Search' }))

    expect(await screen.findByText('needle in local history')).toBeInTheDocument()
  })

  it('refreshes the active channel so durable background replies appear', async () => {
    const baseFetch = vi.mocked(fetch)
    let messageReads = 0
    vi.stubGlobal('fetch', vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
      const path = String(input)
      if (path === `/api/channels/${channel.id}/messages` && !init?.method) {
        messageReads += 1
        return jsonResponse(messageReads === 1 ? [] : [{
          id: 'message-background',
          channel_id: channel.id,
          seq: 1,
          agent_id: 'agent-1',
          role: 'assistant',
          content: 'durable retry completed',
          created_at: '2026-07-16T09:02:01Z',
        }])
      }
      return baseFetch(input, init)
    }))

    render(<LocalApp />)

    expect(await screen.findByRole('heading', { name: '# product-loop' })).toBeInTheDocument()
    expect(
      await screen.findByText('durable retry completed', {}, { timeout: 3_500 }),
    ).toBeInTheDocument()
    expect(messageReads).toBeGreaterThanOrEqual(2)
  })

  it('persists light mode and switches the workspace to Chinese', async () => {
    render(<LocalApp />)

    expect(await screen.findByRole('heading', { name: '# product-loop' })).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: /Appearance: Dark/ }))
    expect(document.documentElement.dataset.localTheme).toBe('light')
    expect(localStorage.getItem('cocli-local-theme')).toBe('light')

    fireEvent.click(screen.getByRole('button', { name: 'Language' }))
    fireEvent.click(screen.getByRole('option', { name: /简体中文/ }))

    expect(await screen.findByRole('heading', { name: '添加 Agent' })).toBeInTheDocument()
    expect(document.documentElement.lang).toBe('zh-CN')
    expect(localStorage.getItem('cocli-local-language')).toBe('zh-CN')
  })

  it('supports keyboard selection in the agent runtime menu', async () => {
    render(<LocalApp />)

    expect(await screen.findByRole('heading', { name: '# product-loop' })).toBeInTheDocument()
    const runtimeSelect = screen.getByRole('button', { name: 'Runtime' })
    fireEvent.keyDown(runtimeSelect, { key: 'ArrowDown' })

    expect(runtimeSelect).toHaveAttribute('aria-expanded', 'true')
    expect(screen.getByRole('listbox', { name: 'Runtime' })).toBeInTheDocument()
    fireEvent.keyDown(runtimeSelect, { key: 'Enter' })
    expect(runtimeSelect).toHaveAttribute('aria-expanded', 'false')
  })

  it('installs a catalog skill from the local Skills workspace', async () => {
    render(<LocalApp />)

    expect(await screen.findByRole('heading', { name: '# product-loop' })).toBeInTheDocument()
    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'builder' } })
    fireEvent.click(screen.getByRole('button', { name: 'Add running agent' }))
    expect(await screen.findByText('builder')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Agents' }))
    fireEvent.click(screen.getByRole('button', { name: 'Skills' }))
    expect(await screen.findByRole('heading', { name: 'Skills workspace' })).toBeInTheDocument()
    expect(await screen.findByRole('heading', { name: 'Governance preview' })).toBeInTheDocument()
    expect(screen.getAllByText('dry-run').length).toBeGreaterThan(0)
    expect(screen.getAllByText('session-effective unknown').length).toBeGreaterThan(0)
    fireEvent.click(screen.getByRole('button', { name: 'Create demo profile' }))
    expect((await screen.findAllByText(/default-governance-profile/)).length).toBeGreaterThan(0)
    fireEvent.click(screen.getByRole('button', { name: 'Bind profile' }))
    expect(await screen.findByText(/machine:machine/)).toBeInTheDocument()
    fireEvent.click(screen.getByRole('tab', { name: 'Lock/Drift' }))
    fireEvent.click(screen.getByRole('button', { name: 'Preview lock' }))
    expect((await screen.findAllByText(/unknown_evidence/)).length).toBeGreaterThan(0)
    fireEvent.click(screen.getByRole('tab', { name: 'Plan Preview' }))
    fireEvent.click(screen.getByRole('button', { name: 'Preview dry-run plan' }))
    expect((await screen.findAllByText('approved but not applied')).length).toBeGreaterThan(0)
    expect((await screen.findAllByText(/manual/)).length).toBeGreaterThan(0)
    fireEvent.click(screen.getByRole('tab', { name: 'Evidence' }))
    expect((await screen.findAllByText(/observed/)).length).toBeGreaterThan(0)
    expect(await screen.findByRole('heading', { name: 'Runtime × Skill inventory' })).toBeInTheDocument()
    expect(screen.getByText(/Neither proves that a running session loaded or activated/)).toBeInTheDocument()
    expect(screen.getByRole('row', { name: /fake supported/ })).toBeInTheDocument()
    expect(screen.getByLabelText('Severity')).toBeInTheDocument()
    expect(screen.getAllByLabelText('Scope').length).toBeGreaterThan(0)
    expect(screen.getByText(/cached snapshot/)).toBeInTheDocument()
    expect(await screen.findByText('Review local changes')).toBeInTheDocument()

    const installButtons = await screen.findAllByRole('button', { name: 'Install' })
    fireEvent.click(installButtons[0])

    expect(await screen.findByText('managed')).toBeInTheDocument()
    expect(screen.getByText('Shell helper')).toBeInTheDocument()
    expect(screen.getByText('disabled by runtime')).toBeInTheDocument()

    fireEvent.change(screen.getByLabelText('Filter Agent skills'), {
      target: { value: 'managed' },
    })
    expect(screen.getAllByText('Reviewer').length).toBeGreaterThan(0)
    expect(screen.queryByText('Shell helper')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'View files' }))
    expect(await screen.findByRole('heading', { name: 'Reviewer' })).toBeInTheDocument()
    expect(await screen.findByText(/Review local changes\./)).toBeInTheDocument()
  })

  it('creates, assigns, updates, and links local tasks', async () => {
    render(<LocalApp />)

    expect(await screen.findByRole('heading', { name: '# product-loop' })).toBeInTheDocument()
    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'builder' } })
    fireEvent.click(screen.getByRole('button', { name: 'Add running agent' }))
    expect(await screen.findByText('builder')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Tasks' }))
    expect(await screen.findByRole('heading', { name: 'Task board' })).toBeInTheDocument()
    fireEvent.change(screen.getByLabelText('New task'), {
      target: { value: 'Prepare release' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Create task' }))

    expect(await screen.findByRole('heading', { name: '#1 Prepare release' })).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Claim task' }))
    expect((await screen.findAllByText('@builder')).length).toBeGreaterThan(0)

    fireEvent.change(screen.getByRole('textbox', { name: 'Progress' }), {
      target: { value: 'Implementation verified' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Save progress' }))
    expect(await screen.findByDisplayValue('Implementation verified')).toBeInTheDocument()

    fireEvent.change(screen.getByLabelText('New task'), {
      target: { value: 'Ship release' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Create task' }))
    expect(await screen.findByRole('heading', { name: '#2 Ship release' })).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Dependencies' }))
    fireEvent.click(screen.getByRole('option', { name: /#1 Prepare release/ }))
    fireEvent.click(screen.getByRole('button', { name: 'Add dependency' }))

    expect(await screen.findByText('#1 Prepare release')).toBeInTheDocument()
  })

  it('creates durable Agent memory from the Channel knowledge context', async () => {
    render(<LocalApp />)

    expect(await screen.findByRole('heading', { name: '# product-loop' })).toBeInTheDocument()
    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'builder' } })
    fireEvent.click(screen.getByRole('button', { name: 'Add running agent' }))
    expect(await screen.findByText('builder')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Memory' }))
    expect(await screen.findByRole('heading', { name: 'Knowledge workspace' })).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'New topic' }))
    fireEvent.change(screen.getByLabelText('Topic slug'), {
      target: { value: 'local loop' },
    })
    fireEvent.change(screen.getByLabelText('Description'), {
      target: { value: 'Local loop decisions' },
    })
    fireEvent.change(screen.getByLabelText('Markdown body'), {
      target: { value: '# Decisions\n\nStay local-first.' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Create topic' }))

    expect(await screen.findByRole('heading', { name: 'local_loop' })).toBeInTheDocument()
    await waitFor(() => {
      expect(screen.getByLabelText('Markdown body')).toHaveValue(
        '# Decisions\n\nStay local-first.',
      )
    })
    expect(screen.queryByRole('button', { name: 'Wiki' })).not.toBeInTheDocument()
  })

  it('inspects runtime history and jumps back to the source message', async () => {
    render(<LocalApp />)

    expect(await screen.findByRole('heading', { name: '# product-loop' })).toBeInTheDocument()
    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'builder' } })
    fireEvent.click(screen.getByRole('button', { name: 'Add running agent' }))
    expect(await screen.findByText('builder')).toBeInTheDocument()

    fireEvent.change(screen.getByLabelText('Task for #product-loop'), {
      target: { value: 'Ship the loop' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Run task' }))
    expect(await screen.findByText('echo: Ship the loop')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Agents' }))
    fireEvent.click(screen.getByRole('button', { name: 'History' }))
    expect(await screen.findByRole('heading', { name: 'Runtime history' })).toBeInTheDocument()
    expect(await screen.findByText('session-1')).toBeInTheDocument()
    fireEvent.click(await screen.findByRole(
      'button',
      { name: /Turn #1/ },
      { timeout: 5_000 },
    ))
    expect(await screen.findByText('Recorded locally.')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Open source message #1' }))
    expect(await screen.findByRole('heading', { name: '# product-loop' })).toBeInTheDocument()
    const sourceMessage = screen.getByText('Ship the loop').closest('article')
    expect(sourceMessage).toHaveClass('history-target')

    fireEvent.click(screen.getByRole('button', { name: 'Agents' }))
    fireEvent.click(screen.getByRole('button', { name: 'History' }))
    fireEvent.click(await screen.findByRole('button', { name: 'Activity' }))
    expect(await screen.findByText('Recording local history')).toBeInTheDocument()
    fireEvent.change(screen.getByLabelText('Filter activity'), {
      target: { value: 'replied' },
    })
    expect(screen.getByText('working')).toBeInTheDocument()
  })
})

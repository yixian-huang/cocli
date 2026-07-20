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
  let mcpApplyRequest: Record<string, unknown> | null = null
  let mcpRollbackRequested = false

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  beforeEach(() => {
    let skillInstalled = false
    let governanceProfileCreated = false
    let governanceBound = false
    let governanceRunCreated = false
    let governanceRunRolledBack = false
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
    let mcpPlanDecision: 'approved' | 'rejected' | null = null
    let memoryTopic: {
      type: 'project'
      topic: string
      description: string
      updated: string
      body: string
      path: string
      version: number
    } | null = null
    mcpApplyRequest = null
    mcpRollbackRequested = false
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
      if (path === '/api/runtimes/mcp/doctor') {
        const evidence = {
          source: 'cursor_cli',
          detail: 'cursor-agent mcp list-tools',
          provesRuntimeLoaded: true,
          provesCurrentSessionVisibility: false,
        }
        return jsonResponse({
          summary: {
            status: 'warning',
            runtimeCount: 1,
            serverCount: 1,
            observationCount: 1,
            diagnosticCount: 1,
            errorCount: 0,
            warningCount: 1,
          },
          inventory: {
            servers: [{
              id: 'srv-docs',
              canonicalName: 'docs',
              definition: { transport: 'stdio', command: 'docs-server' },
              endpointFingerprint: 'sha256:test',
              aliases: ['docs'],
              provenance: [evidence],
              secretRefs: [],
            }],
            bindings: [{ serverId: 'srv-docs', runtime: 'cursor', desiredEnabled: true }],
            observations: [{
              runtime: 'cursor',
              serverId: 'srv-docs',
              alias: 'docs',
              discoverable: true,
              configured: true,
              loaded: true,
              enabled: true,
              approved: false,
              healthy: false,
              startup: 'failed',
              toolCount: 0,
              evidence: [evidence],
              observedAt: '2026-07-19T09:00:00Z',
            }],
            diagnostics: [{
              code: 'approval_missing',
              severity: 'warning',
              runtime: 'cursor',
              serverId: 'srv-docs',
              message: 'MCP server is configured but not approved',
              evidence: [evidence],
              observedAt: '2026-07-19T09:00:00Z',
            }],
            observedAt: '2026-07-19T09:00:00Z',
          },
        })
      }
      if (path === '/api/runtimes/mcp/capabilities') {
        return jsonResponse({
          hash: 'cap-hash-1',
          observedAt: '2026-07-19T09:00:00Z',
          runtimes: [{
            runtime: 'cursor',
            adapter: 'cursor_structured_json_fallback',
            binaryVersion: '1.0.0-test',
            configSchemaVersion: 'cursor.mcpServers.v1',
            destination: '/tmp/workspace/.cursor/mcp.json',
            allowedSubtree: 'mcpServers',
            reloadStrategy: 'new_session_only',
            operations: {
              read_discover: { support: 'supported', reason: 'native probe', evidence: [] },
              add_configure: { support: 'supported', reason: 'structured fallback', evidence: [] },
              reload: { support: 'read_only', reason: 'new sessions only', evidence: [] },
              verify: { support: 'supported', reason: 'fresh readback', evidence: [] },
            },
          }, {
            runtime: 'grok',
            adapter: 'grok_read_only',
            configSchemaVersion: 'grok.mcp_servers.v1',
            destination: '$GROK_HOME/config.toml',
            allowedSubtree: 'mcp_servers',
            reloadStrategy: 'deferred',
            operations: {
              read_discover: { support: 'read_only', reason: 'no binary', evidence: [] },
              add_configure: { support: 'read_only', reason: 'no safe writer', evidence: [] },
            },
          }],
        })
      }
      if (path === '/api/runtimes/mcp/conformance') {
        return jsonResponse({
          schemaVersion: 'mcp-adapter-conformance-summary.v1',
          generatedAt: '2026-07-19T09:00:00Z',
          reportHash: 'conformance-hash-1',
          note: 'Conformance status is separate from live preflight.',
          reports: [{
            schemaVersion: 'mcp-adapter-conformance.v1',
            adapter: {
              runtime: 'cursor',
              adapter: 'cursor_structured_json_fallback',
              adapterVersion: '1.0.0',
              contractVersion: 'mcp-adapter-sdk.v1',
              evidence: [],
            },
            passed: true,
            cases: [{
              name: 'capability_probe_deterministic',
              status: 'passed',
              reason: 'fake adapter conformance passed',
              evidence: [],
            }],
            reportHash: 'cursor-conformance-hash',
          }, {
            schemaVersion: 'mcp-adapter-conformance.v1',
            adapter: {
              runtime: 'grok',
              adapter: 'grok_read_only',
              adapterVersion: '1.0.0',
              contractVersion: 'mcp-adapter-sdk.v1',
              evidence: [],
            },
            passed: true,
            cases: [{
              name: 'unsupported_safe_degrade',
              status: 'passed',
              reason: 'read-only adapter has no writer contract',
              evidence: [],
            }],
            reportHash: 'grok-conformance-hash',
          }],
        })
      }
      if (path === '/api/runtimes/mcp/bundles/export-preview' && init?.method === 'POST') {
        return jsonResponse({
          dryRun: true,
          diagnostics: [{
            code: 'target_rebind_required',
            classification: 'requires_rebind',
            profileRef: 'profile:ops',
            rebindKey: 'machine:1',
            message: 'bundle target must be explicitly rebound',
          }],
          bundle: {
            schemaVersion: 2,
            createdBy: 'desktop-user',
            provenance: {
              producer: 'cocli',
              sourceSchema: 'mcp-governance-phase-3a',
              profileFingerprints: { 'profile:ops': 'fingerprint' },
            },
            profiles: [{
              profileRef: 'profile:ops',
              name: 'Ops baseline',
              sourceVersion: 2,
              servers: [],
            }],
            relativeBindings: [{
              profileRef: 'profile:ops',
              targetType: 'machine',
              targetRef: 'machine:1',
            }],
            portability: [],
            contentHash: 'bundle-hash-1',
          },
        })
      }
      if (path === '/api/runtimes/mcp/bundles/import-preview' && init?.method === 'POST') {
        return jsonResponse({
          canCommit: false,
          audit: {
            id: 'import-1',
            bundleHash: 'bundle-hash-1',
            schemaVersion: 2,
            actor: 'desktop-user',
            status: 'previewed',
            version: 1,
            bundle: JSON.parse(String(init.body)).bundle,
            rebindings: {},
            preview: {
              schemaVersion: 2,
              bundleHash: 'bundle-hash-1',
              diagnostics: [{
                code: 'target_rebind_missing',
                classification: 'requires_rebind',
                profileRef: 'profile:ops',
                rebindKey: 'machine:1',
                message: 'binding target must be explicitly rebound',
              }],
              profileChanges: [{ operation: 'create', profileRef: 'profile:ops' }],
              bindingChanges: [{ operation: 'missing_target_rebind', targetRef: 'machine:1' }],
              approvalImported: false,
              applyImported: false,
              blockingCount: 1,
              capabilityExpectationOnly: true,
            },
            createdAt: '2026-07-19T09:00:00Z',
            updatedAt: '2026-07-19T09:00:00Z',
          },
          preview: {
            schemaVersion: 2,
            bundleHash: 'bundle-hash-1',
            diagnostics: [{
              code: 'target_rebind_missing',
              classification: 'requires_rebind',
              profileRef: 'profile:ops',
              rebindKey: 'machine:1',
              message: 'binding target must be explicitly rebound',
            }],
            profileChanges: [{ operation: 'create', profileRef: 'profile:ops' }],
            bindingChanges: [{ operation: 'missing_target_rebind', targetRef: 'machine:1' }],
            approvalImported: false,
            applyImported: false,
            blockingCount: 1,
            capabilityExpectationOnly: true,
          },
        })
      }
      if (path === '/api/runtimes/mcp/bundles/imports/import-1/rebind' && init?.method === 'POST') {
        const request = JSON.parse(String(init.body))
        return jsonResponse({
          canCommit: true,
          audit: {
            id: 'import-1',
            bundleHash: 'bundle-hash-1',
            schemaVersion: 2,
            actor: 'desktop-user',
            status: 'previewed',
            version: 2,
            bundle: {
              schemaVersion: 2,
              createdBy: 'desktop-user',
              provenance: {
                producer: 'cocli',
                sourceSchema: 'mcp-governance-phase-3a',
                profileFingerprints: { 'profile:ops': 'fingerprint' },
              },
              profiles: [],
              relativeBindings: [],
              portability: [],
              contentHash: 'bundle-hash-1',
            },
            rebindings: request.rebindings,
            preview: {
              schemaVersion: 2,
              bundleHash: 'bundle-hash-1',
              diagnostics: [],
              profileChanges: [{ operation: 'create', profileRef: 'profile:ops' }],
              bindingChanges: [{ operation: 'bind', targetRef: 'machine:1' }],
              approvalImported: false,
              applyImported: false,
              blockingCount: 0,
              capabilityExpectationOnly: true,
            },
            createdAt: '2026-07-19T09:00:00Z',
            updatedAt: '2026-07-19T09:01:00Z',
          },
          preview: {
            schemaVersion: 2,
            bundleHash: 'bundle-hash-1',
            diagnostics: [],
            profileChanges: [{ operation: 'create', profileRef: 'profile:ops' }],
            bindingChanges: [{ operation: 'bind', targetRef: 'machine:1' }],
            approvalImported: false,
            applyImported: false,
            blockingCount: 0,
            capabilityExpectationOnly: true,
          },
        })
      }
      if (path === '/api/runtimes/mcp/profiles') {
        return jsonResponse({
          profiles: [{
            id: 'profile-ops',
            name: 'Ops baseline',
            description: 'Production-safe docs tools',
            version: 2,
            servers: [{
              serverId: 'srv-docs',
              runtime: 'cursor',
              alias: 'docs',
              definition: { transport: 'stdio', command: 'docs-server' },
              desiredEnabled: true,
              allowTools: ['search'],
              denyTools: ['write'],
              approvalMode: 'manual',
              riskOverride: 'high',
              secretRefs: [{ location: 'env', kind: 'token', reference: 'env://DOCS_TOKEN' }],
            }],
            createdAt: '2026-07-19T08:00:00Z',
            updatedAt: '2026-07-19T08:30:00Z',
          }],
        })
      }
      if (path === '/api/runtimes/mcp/bindings') {
        return jsonResponse({
          bindings: [{
            id: 'binding-ops-machine',
            profileId: 'profile-ops',
            target: { targetType: 'machine', targetId: 'machine-local' },
            version: 1,
            createdAt: '2026-07-19T08:05:00Z',
            updatedAt: '2026-07-19T08:05:00Z',
          }],
        })
      }
      if (path === '/api/runtimes/mcp/effective') {
        return jsonResponse({
          target: { machineId: 'machine-local' },
          servers: [{
            serverId: 'srv-docs',
            runtime: 'cursor',
            alias: 'docs',
            definition: { transport: 'stdio', command: 'docs-server' },
            desiredEnabled: true,
            allowTools: ['search'],
            denyTools: ['write'],
            approvalMode: 'manual',
            riskOverride: 'high',
            secretRefs: [{ location: 'env', kind: 'token', reference: 'env://DOCS_TOKEN' }],
            sourceProfileIds: ['profile-ops'],
            sourceProfileNames: ['Ops baseline'],
            inheritedFrom: 'machine',
            highRiskContext: true,
          }],
          conflicts: [{
            runtime: 'cursor',
            serverId: 'srv-docs',
            precedence: 'machine',
            profileIds: ['profile-ops', 'profile-other'],
            reason: 'same-precedence profiles define different desired state',
          }],
          resolution: [{
            profileId: 'profile-ops',
            profileName: 'Ops baseline',
            bindingId: 'binding-ops-machine',
            target: { targetType: 'machine', targetId: 'machine-local' },
            applied: true,
            reason: 'highest precedence binding selected deterministically',
          }],
        })
      }
      if (path === '/api/runtimes/mcp/capabilities') {
        return jsonResponse({
          hash: 'cap-hash-1',
          observedAt: '2026-07-19T09:00:00Z',
          runtimes: [{
            runtime: 'cursor',
            adapter: 'cursor_structured_json_fallback',
            binaryVersion: '1.2.3',
            configSchemaVersion: 'cursor.mcpServers.v1',
            destination: '/tmp/workspace/.cursor/mcp.json',
            allowedSubtree: 'mcpServers',
            reloadStrategy: 'new_session_only',
            operations: {
              read_discover: { support: 'supported', reason: 'native readback available', evidence: [] },
              add_configure: { support: 'supported', reason: 'structured fallback', evidence: [] },
              reload: { support: 'read_only', reason: 'new sessions only', evidence: [] },
              verify: { support: 'supported', reason: 'fresh readback', evidence: [] },
            },
          }],
        })
      }
      if (path === '/api/runtimes/mcp/plans' && init?.method === 'POST') {
        return jsonResponse({
          plan: {
            id: 'plan-1',
            target: { machineId: 'machine-local' },
            effectiveDesiredState: {
              target: { machineId: 'machine-local' },
              servers: [],
              conflicts: [],
              resolution: [],
            },
            actions: [{
              kind: 'approval_required',
              runtime: 'cursor',
              scope: 'machine',
              target: 'machine:machine-local',
              serverId: 'srv-docs',
              serverFingerprint: 'sha256:test',
              before: {
                configured: true,
                enabled: true,
                allowTools: [],
                denyTools: [],
                secretRefCount: 0,
              },
              after: {
                configured: true,
                enabled: true,
                allowTools: ['search'],
                denyTools: ['write'],
                approvalMode: 'manual',
                secretRefCount: 1,
              },
              risk: 'high',
              reason: 'runtime reports server is not approved',
              evidence: [{
                source: 'cursor_cli',
                detail: 'cursor-agent mcp list-tools',
                provesRuntimeLoaded: true,
                provesCurrentSessionVisibility: false,
              }],
              expectedSourceHash: 'obs-hash-1',
              expectedSchemaHash: 'schema-hash-1',
              blocked: true,
            }],
            observationHash: 'obs-hash-1',
            configHash: 'config-hash-1',
            capabilityHash: 'cap-hash-1',
            planHash: 'plan-hash-1',
            generatedAt: '2026-07-19T09:10:00Z',
            dryRun: true,
            applied: false,
          },
          decision: mcpPlanDecision ? {
            id: 'decision-1',
            planId: 'plan-1',
            decision: mcpPlanDecision,
            planHash: 'plan-hash-1',
            observationHash: 'obs-hash-1',
            configHash: 'config-hash-1',
            actor: 'desktop-user',
            decidedAt: '2026-07-19T09:11:00Z',
            expiresAt: '2026-07-19T10:11:00Z',
          } : undefined,
          approvalStatus: mcpPlanDecision ?? 'pending',
          staleReasons: [],
          approvedButNotApplied: mcpPlanDecision === 'approved',
        })
      }
      if (path === '/api/runtimes/mcp/plans/plan-1/preflight') {
        return jsonResponse({
          planId: 'plan-1',
          planHash: 'plan-hash-1',
          capabilityHash: 'cap-hash-1',
          observationHash: 'obs-hash-1',
          configHash: 'config-hash-1',
          executable: false,
          staleReasons: [],
          actions: [{
            actionIndex: 0,
            runtime: 'cursor',
            serverId: 'srv-docs',
            operation: 'verify',
            support: 'supported',
            executable: false,
            reason: 'plan action requires manual handling',
            adapter: 'cursor_structured_json_fallback',
            destination: '/tmp/workspace/.cursor/mcp.json',
            allowedSubtree: 'mcpServers',
            reloadStrategy: 'new_session_only',
            idempotencyKey: 'idem-plan-1-cursor-docs',
            expectedSourceHash: 'obs-hash-1',
            expectedSchemaHash: 'schema-hash-1',
          }],
        })
      }
      if (path === '/api/runtimes/mcp/plans/plan-1/approve' && init?.method === 'POST') {
        mcpPlanDecision = 'approved'
        return (fetch as unknown as (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>)('/api/runtimes/mcp/plans', { method: 'POST' })
      }
      if (path === '/api/runtimes/mcp/plans/plan-1/reject' && init?.method === 'POST') {
        mcpPlanDecision = 'rejected'
        return (fetch as unknown as (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>)('/api/runtimes/mcp/plans', { method: 'POST' })
      }
      if (path === '/api/runtimes/mcp/plans/plan-1/apply' && init?.method === 'POST') {
        mcpApplyRequest = JSON.parse(String(init.body))
        return jsonResponse({
          run: {
            id: 'apply-run-1',
            planId: 'plan-1',
            planHash: 'plan-hash-1',
            observationHash: 'obs-hash-1',
            configHash: 'config-hash-1',
            capabilityHash: 'cap-hash-1',
            actor: 'desktop-user',
            status: 'verified',
            confirmHighRisk: true,
            requestedAt: '2026-07-19T09:12:00Z',
            completedAt: '2026-07-19T09:13:00Z',
            actions: [{
              actionIndex: 0,
              runtime: 'cursor',
              serverId: 'srv-docs',
              status: 'blocked',
              reason: 'approval_required action is never executed by apply',
            }],
            reloads: [{
              runtime: 'cursor',
              status: 'deferred',
              reason: 'active sessions were not restarted',
            }],
            verification: {
              status: 'matched',
              observationHash: 'obs-hash-after',
              mismatches: [],
              writtenConfigHashes: {},
              sessionEffective: 'new_session_required',
            },
            staleReasons: [],
            journal: [{
              sequence: 1,
              actionIndex: 0,
              runtime: 'cursor',
              serverId: 'srv-docs',
              idempotencyKey: 'idem-plan-1-cursor-docs',
              phase: 'preflight',
              attempt: 1,
              reason: 'adapter capability and approved hashes were revalidated',
              evidence: [],
            }],
            preflight: {},
            attempt: 1,
            canRollback: true,
          },
        })
      }
      if (path === '/api/runtimes/mcp/apply-runs/apply-run-1/rollback' && init?.method === 'POST') {
        mcpRollbackRequested = true
        return jsonResponse({
          run: {
            id: 'apply-run-1',
            planId: 'plan-1',
            planHash: 'plan-hash-1',
            observationHash: 'obs-hash-1',
            configHash: 'config-hash-1',
            capabilityHash: 'cap-hash-1',
            actor: 'desktop-user',
            status: 'rolled_back',
            confirmHighRisk: true,
            requestedAt: '2026-07-19T09:12:00Z',
            completedAt: '2026-07-19T09:14:00Z',
            actions: [{
              actionIndex: 0,
              runtime: 'cursor',
              serverId: 'srv-docs',
              status: 'rolled_back',
              reason: 'backup restored',
              backup: {
                id: 'backup-1',
                runtime: 'cursor',
                sourcePath: '/tmp/cursor.json',
                backupPath: '/tmp/backup.json',
                sourceHash: 'source-hash-1',
                backupHash: 'backup-hash-1',
                appliedHash: 'applied-hash-1',
                sourceExisted: true,
              },
            }],
            reloads: [],
            verification: {
              status: 'matched',
              observationHash: 'obs-hash-rollback',
              mismatches: [],
              writtenConfigHashes: {},
              sessionEffective: 'unknown',
            },
            staleReasons: [],
            journal: [],
            preflight: {},
            attempt: 1,
            canRollback: false,
            rollbackStatus: 'rolled_back',
          },
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
      if (path.startsWith('/api/skills/governance/scopes')) {
        return jsonResponse({
          observedAt: '2026-07-19T08:02:30Z',
          capabilities: [{
            runtime: 'fake',
            scope: 'machine',
            rootKind: 'runtime_specific',
            path: '/tmp/cocli-test/fake/skills',
            status: 'supported',
            exists: true,
            writable: true,
            atomicRename: true,
            supported: true,
            evidence: 'runtime-derived canonical target',
            blockedReason: null,
          }, {
            runtime: 'fake',
            scope: 'workspace',
            rootKind: 'runtime_specific',
            path: '/tmp/cocli-test/workspace/.fake/skills',
            status: 'blocked',
            exists: false,
            writable: false,
            atomicRename: false,
            supported: false,
            evidence: 'workspace id required',
            blockedReason: 'missing workspace binding',
          }],
          diagnostics: [],
        })
      }
      if (path === '/api/skills/governance/managed/artifacts' && !init?.method) {
        return jsonResponse([{
          id: 'artifact-1',
          artifactKey: 'sha256:artifact-key',
          artifactKind: 'local_skill',
          sourceProvenance: { kind: 'library', libraryId: 'library-1' },
          contentDigest: 'sha256:content',
          manifestDigest: 'sha256:manifest',
          schemaVersion: 1,
          revision: 'sha256:content',
          storeRelativePath: 'artifacts/content/source/skill',
          artifact: { immutable: true },
          metadata: {},
          version: 1,
          createdAt: '2026-07-19T08:02:30Z',
          referenced: true,
        }])
      }
      if (path === '/api/skills/governance/managed/artifacts/preview' && init?.method === 'POST') {
        return jsonResponse({
          sourceKind: 'library',
          source: { kind: 'library', libraryId: 'library-1' },
          artifactKey: 'sha256:artifact-key',
          contentDigest: 'sha256:content',
          manifestDigest: 'sha256:manifest',
          revision: 'sha256:content',
          storeRelativePath: 'artifacts/content/source/skill',
          previewHash: 'sha256:managed-preview',
          idempotencyKey: 'managed-key-1',
          confirmationNonce: 'managed-nonce-1',
          hazards: [],
          blocked: false,
        })
      }
      if (path === '/api/skills/governance/materializations?scope=machine&scopeId=machine') {
        return jsonResponse([{
          id: 'materialization-1',
          artifactId: 'artifact-1',
          scope: 'machine',
          scopeId: 'machine',
          targetPath: '/tmp/cocli-test/fake/skills/reviewer',
          targetRuntime: 'fake',
          rootKind: 'machine',
          installationMode: 'copy',
          ownership: 'managed',
          contentDigest: 'sha256:content',
          expectedDestination: '/tmp/cocli-test/fake/skills/reviewer',
          expectedFingerprint: 'sha256:target',
          verifyStatus: 'verified',
          receipt: { newSessionRequired: true, sessionEffective: 'unknown' },
          version: 1,
          adoptedAt: null,
          createdAt: '2026-07-19T08:02:30Z',
          updatedAt: '2026-07-19T08:02:30Z',
        }])
      }
      if (path === '/api/skills/governance/adoption/preview' && init?.method === 'POST') {
        return jsonResponse({
          runtime: 'fake',
          scope: 'machine',
          scopeId: 'machine',
          skillName: 'reviewer',
          targetPath: '/tmp/cocli-test/fake/skills/reviewer',
          targetFingerprint: 'sha256:target',
          contentDigest: 'sha256:content',
          manifestDigest: 'sha256:manifest',
          existingOwnership: 'foreign',
          hazards: ['manual_review_required:foreign target'],
          blocked: true,
          previewHash: 'sha256:adoption-preview',
          idempotencyKey: 'adoption-key-1',
          confirmationNonce: 'adoption-nonce-1',
        })
      }
      if (path.startsWith('/api/skills/governance/workspace-lockfile')) {
        return jsonResponse({
          workspaceId: 'workspace-1',
          lockfilePath: '.cocli/skills.lock.json',
          diskHash: 'sha256:disk',
          diskFingerprint: 'sha256:disk-fingerprint',
          stored: {
            id: 'workspace-lockfile-1',
            workspaceId: 'workspace-1',
            lockfilePath: '.cocli/skills.lock.json',
            lockHash: 'sha256:lock',
            expectedDiskFingerprint: 'sha256:disk-fingerprint',
            expectedDiskHash: 'sha256:disk',
            document: {},
            lastBackupPath: null,
            lastBackupHash: null,
            lastReceipt: {},
            restoreMetadata: {},
            version: 1,
            createdAt: '2026-07-19T08:02:30Z',
            updatedAt: '2026-07-19T08:02:30Z',
          },
          exists: true,
        })
      }
      if (path === '/api/skills/governance/gc/preview' && init?.method === 'POST') {
        return jsonResponse({
          previewHash: 'sha256:gc-preview',
          idempotencyKey: 'gc-key-1',
          confirmationNonce: 'gc-nonce-1',
          candidates: [{
            entityType: 'managed_artifact',
            entityId: 'artifact-unreferenced',
            reason: 'unreferenced',
          }],
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
      if (path === '/api/skills/governance/plans/plan-1/apply/preview' && init?.method === 'POST') {
        return jsonResponse({
          plan: {
            id: 'plan-1',
            scope: 'machine',
            scopeId: 'machine',
            plan: { schemaVersion: 1, dryRun: true, applied: false },
            observationHash: 'observation-hash-1',
            desiredHash: 'desired-hash-1',
            status: 'approved',
            version: 1,
            createdAt: '2026-07-19T08:04:00Z',
            updatedAt: '2026-07-19T08:04:00Z',
          },
          dryRun: true,
          applied: false,
          highRisk: true,
          confirmationRequired: true,
          nonceRequired: true,
          confirmationNonce: 'nonce-123',
          idempotencyKey: 'apply-key-1',
          recoveryRequired: true,
          recoveryReasons: ['backup required before applying high-risk changes'],
          lockSnapshotId: 'lock-1',
          backupId: 'backup-1',
          quarantineId: 'quarantine-1',
          effects: [{
            kind: 'backup',
            status: 'pending',
            label: 'Create backup before apply',
            createdId: 'backup-1',
          }, {
            kind: 'quarantine',
            status: 'pending',
            label: 'Quarantine replaced skill files',
            createdId: 'quarantine-1',
          }],
          actions: [],
          staleReasons: [],
        })
      }
      if (path === '/api/skills/governance/plans/plan-1/apply' && init?.method === 'POST') {
        const body = JSON.parse(String(init.body)) as {
          idempotencyKey: string
          confirmationNonce?: string
          confirmHighRisk?: boolean
        }
        if (body.idempotencyKey !== 'apply-key-1' || body.confirmationNonce !== 'nonce-123' || !body.confirmHighRisk) {
          return jsonResponse({ error: 'missing explicit confirmation' }, 409)
        }
        governanceRunCreated = true
        return jsonResponse({
          run: {
            id: 'run-1',
            planId: 'plan-1',
            scope: 'machine',
            scopeId: 'machine',
            status: 'recovery_required',
            phase: 'verify',
            progress: 65,
            message: 'Apply finished; verification requires recovery review',
            dryRun: false,
            applied: true,
            highRisk: true,
            recoveryRequired: true,
            recoveryReasons: ['verification found session-effective unknown'],
            lockSnapshotId: 'lock-1',
            backupId: 'backup-1',
            quarantineId: 'quarantine-1',
            effects: [{
              kind: 'lock',
              status: 'succeeded',
              label: 'Lock snapshot recorded',
              createdId: 'lock-1',
            }, {
              kind: 'backup',
              status: 'succeeded',
              label: 'Backup created',
              createdId: 'backup-1',
            }, {
              kind: 'quarantine',
              status: 'succeeded',
              label: 'Quarantine captured replaced files',
              createdId: 'quarantine-1',
            }],
            actions: [],
            startedAt: '2026-07-19T08:05:00Z',
            updatedAt: '2026-07-19T08:05:10Z',
          },
          applied: true,
          recoveryRequired: true,
        })
      }
      if (path === '/api/skills/governance/runs?scope=machine&scopeId=machine' && !init?.method) {
        return jsonResponse(governanceRunCreated ? [{
          id: 'run-1',
          planId: 'plan-1',
          scope: 'machine',
          scopeId: 'machine',
          status: governanceRunRolledBack ? 'rolled_back' : 'recovery_required',
          phase: governanceRunRolledBack ? 'rollback' : 'verify',
          progress: governanceRunRolledBack ? 100 : 65,
          message: governanceRunRolledBack ? 'Rollback complete' : 'Recovery review required',
          dryRun: false,
          applied: !governanceRunRolledBack,
          highRisk: true,
          recoveryRequired: !governanceRunRolledBack,
          recoveryReasons: governanceRunRolledBack ? [] : ['verification found session-effective unknown'],
          lockSnapshotId: 'lock-1',
          backupId: 'backup-1',
          quarantineId: 'quarantine-1',
          effects: [{
            kind: 'backup',
            status: 'succeeded',
            label: 'Backup created',
            createdId: 'backup-1',
          }],
          actions: [],
          startedAt: '2026-07-19T08:05:00Z',
          updatedAt: '2026-07-19T08:05:10Z',
        }] : [])
      }
      if (path === '/api/skills/governance/runs/run-1/verify' && init?.method === 'POST') {
        return jsonResponse({
          run: {
            id: 'run-1',
            planId: 'plan-1',
            scope: 'machine',
            scopeId: 'machine',
            status: 'recovery_required',
            phase: 'verify',
            progress: 72,
            message: 'Verification requires recovery',
            dryRun: false,
            applied: true,
            highRisk: true,
            recoveryRequired: true,
            recoveryReasons: ['session-effective unknown'],
            effects: [],
            actions: [],
            updatedAt: '2026-07-19T08:06:00Z',
          },
          verified: false,
          recoveryRequired: true,
          reasons: ['session-effective unknown'],
        })
      }
      if (path === '/api/skills/governance/runs/run-1/rollback/preview' && init?.method === 'POST') {
        return jsonResponse({
          run: {
            id: 'run-1',
            planId: 'plan-1',
            scope: 'machine',
            scopeId: 'machine',
            status: 'recovery_required',
            phase: 'rollback',
            progress: 72,
            dryRun: false,
            applied: true,
            highRisk: true,
            recoveryRequired: true,
            recoveryReasons: ['session-effective unknown'],
            effects: [],
            actions: [],
            updatedAt: '2026-07-19T08:06:00Z',
          },
          dryRun: true,
          rollbackRequired: true,
          confirmationRequired: true,
          confirmationNonce: 'rollback-nonce-1',
          idempotencyKey: 'rollback-key-1',
          effects: [{
            kind: 'rollback',
            status: 'pending',
            label: 'Restore from backup',
            createdId: 'backup-1',
          }],
          actions: [],
        })
      }
      if (path === '/api/skills/governance/runs/run-1/rollback' && init?.method === 'POST') {
        const body = JSON.parse(String(init.body)) as {
          idempotencyKey: string
          confirmationNonce?: string
          confirmRollback?: boolean
        }
        if (
          body.idempotencyKey !== 'rollback-key-1'
          || body.confirmationNonce !== 'rollback-nonce-1'
          || !body.confirmRollback
        ) {
          return jsonResponse({ error: 'missing rollback confirmation' }, 409)
        }
        governanceRunRolledBack = true
        return jsonResponse({
          run: {
            id: 'run-1',
            planId: 'plan-1',
            scope: 'machine',
            scopeId: 'machine',
            status: 'rolled_back',
            phase: 'rollback',
            progress: 100,
            message: 'Rollback complete',
            dryRun: false,
            applied: false,
            highRisk: true,
            recoveryRequired: false,
            recoveryReasons: [],
            backupId: 'backup-1',
            quarantineId: 'quarantine-1',
            effects: [{
              kind: 'rollback',
              status: 'succeeded',
              label: 'Restored from backup',
              createdId: 'backup-1',
            }],
            actions: [],
            updatedAt: '2026-07-19T08:07:00Z',
          },
          rolledBack: true,
          recoveryRequired: false,
        })
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
    expect(screen.getByRole('heading', { name: 'Start with a Channel or Agent' })).toBeInTheDocument()
    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'builder' } })
    fireEvent.click(screen.getByRole('button', { name: 'Add running agent' }))

    expect(await screen.findByText('builder')).toBeInTheDocument()
    fireEvent.change(screen.getByLabelText('Message in #product-loop'), {
      target: { value: 'Ship the loop' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Send' }))

    expect(await screen.findByText('echo: Ship the loop')).toBeInTheDocument()
    await waitFor(() => expect(screen.getByText('Ship the loop')).toBeInTheDocument())
    expect(screen.queryByRole('heading', { name: 'Start with a Channel or Agent' })).not.toBeInTheDocument()
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
    fireEvent.click(screen.getByText('Optional resource handles'))
    fireEvent.change(screen.getByLabelText('Resource location'), {
      target: { value: '/tmp/general-purpose-work' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Attach handle' }))
    expect(await screen.findByText('directory workspace · directory')).toBeInTheDocument()

    fireEvent.change(screen.getByPlaceholderText('Describe the requirement for this Agent…'), {
      target: { value: 'Research a non-code topic' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Send' }))
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

  it('documents durable state backup and portable CLI restore in Settings', async () => {
    render(<LocalApp />)

    expect(await screen.findByRole('heading', { name: '# product-loop' })).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
    expect(await screen.findByRole('heading', { name: 'Durable state' })).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Portable backup (CLI)' })).toBeInTheDocument()
    expect(screen.getByText(/cocli backup --portable/)).toBeInTheDocument()
    const backupLinks = screen.getAllByRole('link', { name: 'Download application-state backup' })
    expect(backupLinks.length).toBeGreaterThanOrEqual(1)
    expect(backupLinks[0]).toHaveAttribute('href', '/api/backups/state')
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

    expect(await screen.findByRole('heading', { name: '邀请 Agent' })).toBeInTheDocument()
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
    fireEvent.click(screen.getByRole('tab', { name: 'Scopes' }))
    expect(await screen.findByText(/runtime-derived canonical target/)).toBeInTheDocument()
    expect(await screen.findByText(/missing workspace binding/)).toBeInTheDocument()
    fireEvent.click(screen.getByRole('tab', { name: 'Managed Store' }))
    expect(await screen.findByText(/local_skill/)).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Preview artifact' }))
    expect(await screen.findByText('ready')).toBeInTheDocument()
    expect((await screen.findAllByText(/artifacts\/content\/source\/skill/)).length).toBeGreaterThan(0)
    expect(screen.getByDisplayValue('managed-key-1')).toBeInTheDocument()
    expect(screen.getByDisplayValue('managed-nonce-1')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('tab', { name: 'Materializations' }))
    expect(await screen.findByText(/artifact_stored=yes/)).toBeInTheDocument()
    expect(await screen.findByText(/session_effective=unknown/)).toBeInTheDocument()
    fireEvent.click(screen.getByRole('tab', { name: 'Adoption' }))
    fireEvent.change(screen.getByLabelText('Skill name'), { target: { value: 'reviewer' } })
    fireEvent.click(screen.getByRole('button', { name: 'Preview adoption' }))
    expect(await screen.findByText(/manual_review_required:foreign target/)).toBeInTheDocument()
    expect(await screen.findByText(/blocked\/manual/)).toBeInTheDocument()
    fireEvent.click(screen.getByRole('tab', { name: 'Workspace Lockfile' }))
    fireEvent.change(screen.getByLabelText('Workspace target'), { target: { value: 'workspace-1' } })
    fireEvent.click(screen.getByRole('button', { name: 'Inspect lockfile' }))
    expect(await screen.findByText(/stored snapshot: v1/)).toBeInTheDocument()
    fireEvent.click(screen.getByRole('tab', { name: 'GC' }))
    fireEvent.click(screen.getByRole('button', { name: 'Preview GC' }))
    expect(await screen.findByText('unreferenced')).toBeInTheDocument()
    expect(screen.getByDisplayValue('gc-key-1')).toBeInTheDocument()
    expect(screen.getByDisplayValue('gc-nonce-1')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('tab', { name: 'Profiles' }))
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
    fireEvent.click(screen.getByRole('tab', { name: 'Apply/Recovery' }))
    fireEvent.click(screen.getAllByRole('button', { name: 'Preview apply' })[0])
    expect((await screen.findAllByText('high risk')).length).toBeGreaterThan(0)
    expect((await screen.findAllByText('recovery required')).length).toBeGreaterThan(0)
    expect(await screen.findByText(/Backup created|Create backup before apply/)).toBeInTheDocument()
    expect(screen.getByDisplayValue('apply-key-1')).toBeInTheDocument()
    expect(screen.getByDisplayValue('nonce-123')).toBeInTheDocument()
    fireEvent.click(screen.getByLabelText('I explicitly confirm this apply operation.'))
    fireEvent.click(screen.getByRole('button', { name: 'Apply with confirmation' }))
    expect(await screen.findByText(/Apply finished/)).toBeInTheDocument()
    expect(await screen.findByText(/Lock snapshot recorded/)).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Verify' }))
    expect((await screen.findAllByText(/session-effective unknown/)).length).toBeGreaterThan(0)
    fireEvent.click(screen.getByRole('button', { name: 'Preview rollback' }))
    expect(await screen.findByText(/Restore from backup/)).toBeInTheDocument()
    const idempotencyInputs = screen.getAllByLabelText('Idempotency key')
    fireEvent.change(idempotencyInputs[idempotencyInputs.length - 1], {
      target: { value: 'rollback-key-1' },
    })
    fireEvent.click(screen.getByLabelText('I explicitly confirm this rollback operation.'))
    fireEvent.click(screen.getByRole('button', { name: 'Rollback' }))
    expect(await screen.findByText(/Rollback complete/)).toBeInTheDocument()
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
  }, 15_000)

  it('shows the read-only MCP runtime matrix and evidence', async () => {
    render(<LocalApp />)

    expect(await screen.findByRole('heading', { name: '# product-loop' })).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Agents' }))
    fireEvent.click(screen.getByRole('button', { name: 'MCP' }))

    expect(await screen.findByRole('heading', { name: 'MCP inventory and doctor' })).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Runtime × Server matrix' })).toBeInTheDocument()
    expect(screen.getByRole('row', { name: /cursor D✓ C✓ L✓ E✓ P× A· H× S· I·/ })).toBeInTheDocument()
    expect(screen.getByText(/cursor-agent mcp list-tools/)).toBeInTheDocument()
    expect(screen.getByText(/approval_missing/)).toBeInTheDocument()
    expect(screen.getByText(/Phase 2C applies only valid approvals/)).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Runtime adapter capabilities' })).toBeInTheDocument()
    expect(screen.getAllByText(/cursor_structured_json_fallback/).length).toBeGreaterThan(0)
    expect(screen.getByRole('heading', { name: 'Portability' })).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Adapter conformance' })).toBeInTheDocument()
    expect(screen.getByText(/conformance-hash-1/)).toBeInTheDocument()
    expect(screen.getByText(/fake adapter conformance passed/)).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Profiles' })).toBeInTheDocument()
    expect(screen.getByText('Ops baseline')).toBeInTheDocument()
    expect(screen.getAllByText(/machine:machine-local/).length).toBeGreaterThan(0)
    expect(screen.getByRole('heading', { name: 'Effective desired state' })).toBeInTheDocument()
    expect(screen.getByText(/same-precedence profiles/)).toBeInTheDocument()
    expect(screen.getByText(/high-risk context/)).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Export preview' }))
    expect(await screen.findByText(/Bundle hash: bundle-hash-1/)).toBeInTheDocument()
    expect(screen.getByText(/target_rebind_required/)).toBeInTheDocument()
    expect((screen.getByLabelText('Bundle JSON') as HTMLTextAreaElement).value).toContain('bundle-hash-1')
    fireEvent.click(screen.getByRole('button', { name: 'Import preview' }))
    expect(await screen.findByText(/Import audit: import-1/)).toBeInTheDocument()
    expect(screen.getByText(/target_rebind_missing/)).toBeInTheDocument()
    expect(screen.getByText(/Can commit: no/)).toBeInTheDocument()
    fireEvent.change(screen.getByLabelText('machine:1'), {
      target: { value: 'machine-local' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Apply rebindings' }))
    expect(await screen.findByText(/Can commit: yes/)).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Generate dry-run plan' }))

    expect(await screen.findByText(/Approval status: pending/)).toBeInTheDocument()
    expect(screen.getByText(/Preflight:/)).toBeInTheDocument()
    expect(screen.getByText(/approval_required/)).toBeInTheDocument()
    expect(screen.getByText(/high · blocked/)).toBeInTheDocument()
    expect(screen.getByText(/runtime reports server is not approved/)).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Approve plan' }))

    expect(await screen.findByText(/Approval status: approved/)).toBeInTheDocument()
    expect(screen.getByText('approved but not applied')).toBeInTheDocument()
    expect(screen.getByText(/Approval expires:/)).toBeInTheDocument()
    expect(screen.getByText(/Manual, blocked, auth-required, and unsupported actions are never executed/)).toBeInTheDocument()
    expect(screen.getByText(/0 executable action/)).toBeInTheDocument()

    const applyButton = screen.getByRole('button', { name: 'Apply approved plan' })
    expect(applyButton).toBeDisabled()
    fireEvent.click(screen.getByLabelText(/I confirm high-risk MCP configuration changes/))
    expect(applyButton).toBeEnabled()
    fireEvent.click(applyButton)

    expect(await screen.findByText(/Apply status: verified/)).toBeInTheDocument()
    expect(screen.getByText(/blocked · cursor · srv-docs/)).toBeInTheDocument()
    expect(screen.getByText(/approval_required action is never executed by apply/)).toBeInTheDocument()
    expect(screen.getByText(/deferred · cursor/)).toBeInTheDocument()
    expect(screen.getByText(/active sessions were not restarted/)).toBeInTheDocument()
    expect(screen.getByText(/Verify: matched/)).toBeInTheDocument()
    expect(screen.getByText(/Durable journal/)).toBeInTheDocument()
    expect(screen.getByText(/adapter capability and approved hashes were revalidated/)).toBeInTheDocument()
    expect(mcpApplyRequest).toMatchObject({
      planHash: 'plan-hash-1',
      observationHash: 'obs-hash-1',
      configHash: 'config-hash-1',
      confirmHighRisk: true,
    })

    fireEvent.click(screen.getByRole('button', { name: 'Rollback' }))
    expect(await screen.findByText(/Apply status: rolled_back/)).toBeInTheDocument()
    expect(screen.getByText(/Rollback status: rolled_back/)).toBeInTheDocument()
    expect(screen.getByText(/backup restored/)).toBeInTheDocument()
    expect(mcpRollbackRequested).toBe(true)
  })

  it('creates, assigns, updates, and links local tasks', async () => {
    render(<LocalApp />)

    expect(await screen.findByRole('heading', { name: '# product-loop' })).toBeInTheDocument()
    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'builder' } })
    fireEvent.click(screen.getByRole('button', { name: 'Add running agent' }))
    expect(await screen.findByText('builder')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Coordination' }))
    expect(await screen.findByRole('heading', { name: 'Coordination' })).toBeInTheDocument()
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

    fireEvent.change(screen.getByLabelText('Message in #product-loop'), {
      target: { value: 'Ship the loop' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Send' }))
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

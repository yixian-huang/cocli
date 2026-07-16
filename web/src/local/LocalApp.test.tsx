import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
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
  created_at: '2026-07-16T09:00:00Z',
}

function jsonResponse(body: unknown, status = 200) {
  return Promise.resolve(new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  }))
}

describe('LocalApp', () => {
  beforeEach(() => {
    let skillInstalled = false
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
      if (path === '/api/runtimes/compatibility') {
        return jsonResponse({ fake: 'supported' })
      }
      if (path === '/api/channels' && !init?.method) return jsonResponse([channel])
      if (path === '/api/agents' && !init?.method) return jsonResponse([])
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
      if (path === '/api/agents' && init?.method === 'POST') {
        return jsonResponse({
          id: 'agent-1',
          channel_id: channel.id,
          name: 'builder',
          runtime: 'fake',
          model: 'test-model',
          status: 'running',
          created_at: '2026-07-16T09:01:00Z',
        }, 201)
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
    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'builder' } })
    fireEvent.click(screen.getByRole('button', { name: 'Add running agent' }))

    expect(await screen.findByText('builder')).toBeInTheDocument()
    fireEvent.change(screen.getByLabelText('Task for #product-loop'), {
      target: { value: 'Ship the loop' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Run task' }))

    expect(await screen.findByText('echo: Ship the loop')).toBeInTheDocument()
    await waitFor(() => expect(screen.getByText('Ship the loop')).toBeInTheDocument())
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

    fireEvent.click(screen.getByRole('button', { name: 'Skills' }))
    expect(await screen.findByRole('heading', { name: 'Skills workspace' })).toBeInTheDocument()
    expect(await screen.findByText('Review local changes')).toBeInTheDocument()

    const installButtons = await screen.findAllByRole('button', { name: 'Install' })
    fireEvent.click(installButtons[0])

    expect(await screen.findByText('managed')).toBeInTheDocument()
    expect(screen.getByText('Shell helper')).toBeInTheDocument()

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
})

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { useAgentSkillStore } from './agentSkillStore'
import * as client from '@/api/client'

beforeEach(() => {
  useAgentSkillStore.setState({
    skillsByAgent: {},
    compatibility: null,
    loadingByAgent: {},
    errorByAgent: {},
  })
})

describe('agentSkillStore', () => {
  it('fetchForAgent populates skillsByAgent', async () => {
    vi.spyOn(client.agentSkills, 'list').mockResolvedValue({
      skills: [{ name: 'wikic', userInvocable: false, type: 'workspace', state: 'managed' }],
    })
    await useAgentSkillStore.getState().fetchForAgent('a1')
    expect(useAgentSkillStore.getState().skillsByAgent['a1']).toHaveLength(1)
  })

  it('install triggers optimistic refetch', async () => {
    const installSpy = vi.spyOn(client.agentSkills, 'install').mockResolvedValue({
      installId: 'i1', installPath: '.claude/skills/wikic',
    })
    const listSpy = vi.spyOn(client.agentSkills, 'list').mockResolvedValue({ skills: [] })
    await useAgentSkillStore.getState().install('a1', 'lib-1')
    expect(installSpy).toHaveBeenCalledWith('a1', 'lib-1')
    expect(listSpy).toHaveBeenCalled()
  })

  it('uninstall triggers refetch', async () => {
    const unSpy = vi.spyOn(client.agentSkills, 'uninstall').mockResolvedValue({ ok: true })
    const listSpy = vi.spyOn(client.agentSkills, 'list').mockResolvedValue({ skills: [] })
    await useAgentSkillStore.getState().uninstall('a1', 'i1')
    expect(unSpy).toHaveBeenCalled()
    expect(listSpy).toHaveBeenCalled()
  })

  it('loadCompatibility caches matrix', async () => {
    const spy = vi.spyOn(client.runtimes, 'compatibility').mockResolvedValue({
      claude: 'supported', chatrs: 'unsupported',
    })
    await useAgentSkillStore.getState().loadCompatibility()
    expect(useAgentSkillStore.getState().compatibility?.claude).toBe('supported')
    await useAgentSkillStore.getState().loadCompatibility() // should use cache
    expect(spy).toHaveBeenCalledTimes(1)
  })
})

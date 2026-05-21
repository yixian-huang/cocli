import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { MemoryPanel } from './MemoryPanel'
import { useViewStore } from '@/stores/viewStore'

vi.mock('../MemoryTab', () => ({
  MemoryTab: ({ agentId }: { agentId: string }) => (
    <div data-testid="memory-tab-stub">memory for {agentId}</div>
  ),
}))

beforeEach(() => {
  useViewStore.setState({
    activeAgentId: 'a',
    quotedMessage: null,
    agentSubview: {},
    activeDrawer: 'memory',
  })
})

afterEach(() => cleanup())

describe('MemoryPanel', () => {
  it('embeds MemoryTab for the given agent', () => {
    render(<MemoryPanel agentId="a" />)
    expect(screen.getByTestId('memory-tab-stub')).toHaveTextContent('memory for a')
  })

  it('Edit in Settings switches subview and closes drawer', () => {
    render(<MemoryPanel agentId="a" />)
    fireEvent.click(screen.getByRole('button', { name: /edit in settings/i }))
    expect(useViewStore.getState().getSubview('a')).toBe('settings')
    expect(useViewStore.getState().activeDrawer).toBeNull()
  })
})

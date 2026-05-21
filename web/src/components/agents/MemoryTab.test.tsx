import { describe, it, expect, beforeEach } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import { useMemoryStore } from '@/stores/memoryStore'
import { MemoryTab } from './MemoryTab'

beforeEach(() => {
  useMemoryStore.setState({
    entries: { 'agent:a1': '- [user_alice](user_alice.md) — Alice frontend prefs\n- [feedback_bob](feedback_bob.md) — Bob feedback\n' } as any,
    topics: {} as any,
  })
})

describe('MemoryTab', () => {
  it('renders agent memory index entries', async () => {
    render(<MemoryTab agentId="a1" />)
    expect(await screen.findByText(/Alice frontend prefs/)).toBeInTheDocument()
    expect(await screen.findByText(/Bob feedback/)).toBeInTheDocument()
  })

  it('type filter narrows entries', async () => {
    render(<MemoryTab agentId="a1" />)
    await waitFor(() => screen.getByText(/Bob feedback/))
    const userBtn = screen.getByRole('button', { name: /^user$/i })
    userBtn.click()
    await waitFor(() => {
      expect(screen.queryByText(/Bob feedback/)).not.toBeInTheDocument()
    })
    expect(screen.getByText(/Alice frontend prefs/)).toBeInTheDocument()
  })

  it('shows empty placeholder before selecting a topic', () => {
    render(<MemoryTab agentId="a1" />)
    expect(screen.getByText(/Select a topic/i)).toBeInTheDocument()
  })
})

import { describe, it, expect, beforeEach } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import { useMemoryStore } from '@/stores/memoryStore'
import type { MemoryTopic } from '@/api/client'
import { ChannelMemoryPanel } from './ChannelMemoryPanel'

beforeEach(() => {
  useMemoryStore.setState({
    entries: { 'channel:c1': '- [project_apollo](project_apollo.md) — Apollo plan\n- [reference_links](reference_links.md) — External links\n' } as unknown as Record<string, string>,
    topics: {} as unknown as Record<string, MemoryTopic>,
  })
})

describe('ChannelMemoryPanel', () => {
  it('renders channel memory index entries', async () => {
    render(<ChannelMemoryPanel channelId="c1" />)
    expect(await screen.findByText(/Apollo plan/)).toBeInTheDocument()
    expect(await screen.findByText(/External links/)).toBeInTheDocument()
  })

  it('type filter narrows entries', async () => {
    render(<ChannelMemoryPanel channelId="c1" />)
    await waitFor(() => screen.getByText(/External links/))
    const projectBtn = screen.getByRole('button', { name: /^project$/i })
    projectBtn.click()
    await waitFor(() => {
      expect(screen.queryByText(/External links/)).not.toBeInTheDocument()
    })
    expect(screen.getByText(/Apollo plan/)).toBeInTheDocument()
  })

  it('shows empty placeholder before selecting a topic', () => {
    render(<ChannelMemoryPanel channelId="c1" />)
    expect(screen.getByText(/Select a topic/i)).toBeInTheDocument()
  })
})

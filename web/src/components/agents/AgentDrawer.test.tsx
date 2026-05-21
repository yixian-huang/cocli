import type { ReactNode } from 'react'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { AgentDrawer } from './AgentDrawer'
import { useViewStore } from '@/stores/viewStore'

beforeEach(() => {
  useViewStore.setState({
    activeAgentId: 'a',
    quotedMessage: null,
    agentSubview: {},
    activeDrawer: null,
    historyDrawerSegment: null,
  })
})

afterEach(() => cleanup())

function renderInRouter(ui: ReactNode) {
  return render(<MemoryRouter>{ui}</MemoryRouter>)
}

describe('AgentDrawer', () => {
  it('renders nothing when activeDrawer is null', () => {
    const { container } = renderInRouter(<AgentDrawer agentId="a" />)
    expect(container.firstChild).toBeNull()
  })

  it('renders LivePanel when activeDrawer is "live"', () => {
    useViewStore.setState({ activeDrawer: 'live' })
    renderInRouter(<AgentDrawer agentId="a" />)
    expect(screen.getByTestId('drawer-live')).toBeInTheDocument()
  })

  it('renders HistoryPanel when activeDrawer is "history"', () => {
    useViewStore.setState({ activeDrawer: 'history' })
    renderInRouter(<AgentDrawer agentId="a" />)
    expect(screen.getByTestId('drawer-history')).toBeInTheDocument()
  })

  it('renders MemoryPanel when activeDrawer is "memory"', () => {
    useViewStore.setState({ activeDrawer: 'memory' })
    renderInRouter(<AgentDrawer agentId="a" />)
    expect(screen.getByTestId('drawer-memory')).toBeInTheDocument()
  })

  it('close button clears activeDrawer', () => {
    useViewStore.setState({ activeDrawer: 'live' })
    renderInRouter(<AgentDrawer agentId="a" />)
    fireEvent.click(screen.getByRole('button', { name: /close drawer/i }))
    expect(useViewStore.getState().activeDrawer).toBeNull()
  })

  it('Esc key clears activeDrawer', () => {
    useViewStore.setState({ activeDrawer: 'memory' })
    renderInRouter(<AgentDrawer agentId="a" />)
    fireEvent.keyDown(window, { key: 'Escape' })
    expect(useViewStore.getState().activeDrawer).toBeNull()
  })
})

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { CreateAgentDialog } from './CreateAgentDialog'
import { useDialogStore, resetDialogStore } from '@/stores/dialogStore'
import * as client from '@/api/client'

describe('<CreateAgentDialog> dialogStore wiring', () => {
  beforeEach(() => {
    resetDialogStore()
    vi.spyOn(client.agents, 'runtimes').mockResolvedValue([])
  })
  afterEach(() => {
    cleanup()
    vi.restoreAllMocks()
  })

  it('opens via openCreateAgent and closes via Cancel', async () => {
    render(<CreateAgentDialog />)
    useDialogStore.getState().openCreateAgent({ zoneId: 'z1' })
    await waitFor(() => screen.getByLabelText(/^name$/i))
    fireEvent.click(screen.getByRole('button', { name: /cancel/i }))
    expect(useDialogStore.getState().active).toBe(null)
  })
})

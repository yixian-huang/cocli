import { useCallback } from 'react'
import { useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useViewStore } from '@/stores/viewStore'

/** Leave agent view; navigates to agentReturnTo when the agent was opened from another route. */
export function useExitAgentView() {
  const navigate = useNavigate()
  const clearActiveAgent = useViewStore((s) => s.clearActiveAgent)

  return useCallback(() => {
    const returnTo = useViewStore.getState().agentReturnTo
    clearActiveAgent()
    if (returnTo) navigate(returnTo)
  }, [clearActiveAgent, navigate])
}

export function useAgentBackLabel() {
  const { t } = useTranslation()
  const agentReturnTo = useViewStore((s) => s.agentReturnTo)
  return agentReturnTo?.includes('/daemons')
    ? t('workspace.back.daemons')
    : t('workspace.back.channel')
}

import { useEffect } from 'react'
import { useLocation, useParams } from 'react-router-dom'
import { useViewStore } from '@/stores/viewStore'
import { useChannelStore } from '@/stores/channelStore'

export function AgentRoute() {
  const { id } = useParams<{ id: string }>()
  const location = useLocation()
  const setActiveAgent = useViewStore((s) => s.setActiveAgent)
  const setActiveChannel = useChannelStore((s) => s.setActiveChannel)
  const returnTo = (location.state as { returnTo?: string } | null)?.returnTo

  useEffect(() => {
    if (!id) return
    setActiveChannel('')
    setActiveAgent(id, returnTo)
  }, [id, returnTo, setActiveAgent, setActiveChannel])

  return null
}

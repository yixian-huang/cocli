import { useEffect } from 'react'
import { useParams } from 'react-router-dom'
import { useChannelStore } from '@/stores/channelStore'
import { useViewStore } from '@/stores/viewStore'
import { useWorkspacePanelStore } from '@/stores/workspacePanelStore'

export function ChannelRoute() {
  const { channelId, id: msgId } = useParams<{ channelId: string; id?: string }>()
  const channels = useChannelStore((s) => s.channels)
  const dmChannels = useChannelStore((s) => s.dmChannels)
  const setActiveChannel = useChannelStore((s) => s.setActiveChannel)
  const clearActiveAgent = useViewStore((s) => s.clearActiveAgent)
  const setPanel = useWorkspacePanelStore((s) => s.setPanel)

  useEffect(() => {
    clearActiveAgent()
    if (!channelId) return
    const all = [...channels, ...dmChannels]
    const channel = all.find((c) => c.id === channelId)
    if (channel) {
      setPanel('chat')
      setActiveChannel(channel.id)
    }
  }, [channelId, channels, dmChannels, setActiveChannel, clearActiveAgent, setPanel])

  useEffect(() => {
    if (msgId) {
      window.dispatchEvent(new CustomEvent('scroll-to-message', { detail: { msgId } }))
    }
  }, [msgId])

  return null
}

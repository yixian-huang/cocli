import { useState, useEffect } from 'react'
import { useChannelStore } from '@/stores/channelStore'
import { channels as channelsApi, exportData, getApiKey } from '@/api/client'
import { toast, toastError } from '@/stores/toastStore'
import type { Channel } from '@/lib/types'
import { Save, Download } from 'lucide-react'
import { Button, Input, Textarea } from '@/components/ui'

interface Props {
  channel: Channel
  channelId: string
}

export function ChannelSettingsForm({ channel, channelId }: Props) {
  const fetchChannels = useChannelStore((s) => s.fetchChannels)

  const [displayName, setDisplayName] = useState(channel.displayName || channel.name)
  const [description, setDescription] = useState(channel.description || '')
  const [saving, setSaving] = useState(false)

  useEffect(() => {
    setDisplayName(channel.displayName || channel.name)
    setDescription(channel.description || '')
  }, [channel])

  const handleSave = async () => {
    if (!displayName.trim()) return
    setSaving(true)
    try {
      await channelsApi.update(channelId, {
        displayName: displayName.trim(),
        description: description.trim(),
      })
      await fetchChannels()
      toast('Channel updated', 'success')
    } catch (err) {
      toastError(err instanceof Error ? err.message : 'Failed to update')
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="space-y-4">
      <Input
        label="Display Name"
        type="text"
        value={displayName}
        onChange={(e) => setDisplayName(e.target.value)}
      />
      <Textarea
        label="Description"
        value={description}
        onChange={(e) => setDescription(e.target.value)}
        rows={3}
        placeholder="What's this channel about?"
      />
      <Button
        variant="primary"
        className="w-full"
        onClick={handleSave}
        disabled={saving || !displayName.trim()}
      >
        <Save className="h-3.5 w-3.5" />
        {saving ? 'Saving...' : 'Save Changes'}
      </Button>

      <div className="border-t pt-4 mt-4">
        <h4 className="text-xs font-medium text-muted-foreground mb-2">Export Data</h4>
        <div className="space-y-2">
          {(['json', 'csv'] as const).map((fmt) => (
            <div key={fmt} className="flex gap-2">
              <Button
                variant="secondary"
                className="flex-1"
                onClick={() => {
                  const url = exportData.messagesUrl(channelId, fmt)
                  fetch(url, { headers: { 'X-API-Key': getApiKey() } })
                    .then((r) => r.blob())
                    .then((b) => {
                      const a = document.createElement('a')
                      a.href = URL.createObjectURL(b)
                      a.download = `messages.${fmt}`
                      a.click()
                    })
                }}
              >
                <Download className="h-3 w-3" />
                Messages .{fmt.toUpperCase()}
              </Button>
              <Button
                variant="secondary"
                className="flex-1"
                onClick={() => {
                  const url = exportData.tasksUrl(channelId, fmt)
                  fetch(url, { headers: { 'X-API-Key': getApiKey() } })
                    .then((r) => r.blob())
                    .then((b) => {
                      const a = document.createElement('a')
                      a.href = URL.createObjectURL(b)
                      a.download = `tasks.${fmt}`
                      a.click()
                    })
                }}
              >
                <Download className="h-3 w-3" />
                Tasks .{fmt.toUpperCase()}
              </Button>
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}

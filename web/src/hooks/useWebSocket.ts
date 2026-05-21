import { useEffect, useRef } from 'react'
import { getApiKey } from '@/api/client'
import { useMessageStore } from '@/stores/messageStore'
import { useChannelStore } from '@/stores/channelStore'
import { useAgentStore } from '@/stores/agentStore'
import { useTaskStore } from '@/stores/taskStore'
import { useWSStore } from '@/stores/wsStore'
import { usePresenceStore } from '@/stores/presenceStore'
import { useUserStore } from '@/stores/userStore'
import { useZoneStore } from '@/stores/zoneStore'
import { defaultTitle, storageKey } from '@/brand'
import { useDevToolsStore } from '@/stores/devToolsStore'
import { useSidebarPrefsStore } from '@/stores/sidebarPrefsStore'
import type { Agent, AgentAttentionState, MachineVersionStatus, Message, PriorityClass, Task, TrajectoryEntry, Turn, WSEvent } from '@/lib/types'
import { useMachineStatusStore } from '@/stores/machineStatusStore'
import { toastError } from '@/stores/toastStore'
import { applyPrefsFromServer } from '@/stores/prefsStore'

type WSEventMap = {
  'message:new': Message
  'message:update': Message
  'task:update': Task
  'channel:updated': { id: string; archived?: boolean; displayName?: string; description?: string }
  'prefs:updated': { prefs: Record<string, unknown> }
  'agent:status': { agentId: string; status: Agent['status']; errorDetail?: string }
  'agent:activity': { agentId: string; activity: string; detail?: string; trajectory?: string[]; lastInputTokens?: number; totalOutputTokens?: number; contextWindow?: number; totalCostUSD?: number; turnCount?: number; entries?: TrajectoryEntry[]; launchGeneration?: number; attentionState?: AgentAttentionState; focusTaskId?: string; focusScope?: string; focusSince?: number; priorityClass?: PriorityClass; preempted?: boolean }
  'user:presence': { userId: string; online: boolean }
  'agent:turn': { agentId: string; sessionId: string; turnNumber: number; entries: TrajectoryEntry[]; inputTokens?: number; outputTokens?: number; costUsd?: number; contextWindow?: number; channelName?: string; contextUsagePct?: number; launchGeneration?: number }
  'agent:session': { agentId: string; sessionId: string; channelId?: string; isNew?: boolean; resumedFrom?: string; activeSessions?: number; launchId?: string; launchGeneration?: number; sessionType?: string; scope?: string }
  'agent:session:end': { agentId: string; sessionId: string; endReason?: string; turnCount?: number; launchId?: string; launchGeneration?: number; sessionType?: string; scope?: string; inputTokens?: number; outputTokens?: number; costUsd?: number; contextWindow?: number }
  'thread:update': { id: string; done: boolean }
  'thread:activity': { threadId: string; parentMessageId: string; parentChannelId: string; replyCount: number; lastReply: { senderName: string; content: string; timestamp: string } }
  'agent:stop:error': { agentId: string; error: string }
  'machine:updated': { machineId: string; hostname: string; daemonVersion: string; versionStatus: MachineVersionStatus }
}

function sendBrowserNotification(msg: Message) {
  if (Notification.permission !== 'granted') return
  if (document.hasFocus()) return
  const body = msg.content.length > 100 ? msg.content.slice(0, 100) + '...' : msg.content
  new Notification(msg.senderName, { body, tag: msg.id })
}

function updateTitleUnread() {
  const state = useChannelStore.getState()
  const total = [...state.channels, ...state.dmChannels].reduce((sum, c) => sum + (c.unreadCount || 0), 0)
  document.title = total > 0 ? `(${total}) ${defaultTitle}` : defaultTitle
}

const MIN_BACKOFF = 1000
const MAX_BACKOFF = 30000

export function useWebSocket() {
  const wsRef = useRef<WebSocket | null>(null)
  const attemptRef = useRef(0)
  useEffect(() => {
    let mounted = true
    let timer: ReturnType<typeof setTimeout>

    function connect() {
      const key = getApiKey()
      if (!key) return

      const zoneId = useZoneStore.getState().activeZoneId
      useWSStore.getState().setStatus('connecting')
      const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
      const ws = new WebSocket(`${protocol}//${window.location.host}/ws?key=${key}${zoneId ? `&zone=${zoneId}` : ''}`)
      wsRef.current = ws

      ws.onopen = () => {
        const wasReconnect = attemptRef.current > 0
        attemptRef.current = 0
        useWSStore.getState().setStatus('connected')
        usePresenceStore.getState().fetchPresence()
        // Re-sync unread counts after reconnect
        useChannelStore.getState().fetchChannels()
        useChannelStore.getState().fetchDMs()
        // Backfill missed messages for channels we've already loaded
        if (wasReconnect) {
          useMessageStore.getState().backfillMessages()
        }
        if ('Notification' in window && Notification.permission === 'default') {
          Notification.requestPermission()
        }
      }

      ws.onmessage = (ev) => {
        try {
          const event = JSON.parse(ev.data) as WSEvent
          if (!event.data) return

          switch (event.type) {
            case 'message:new': {
              const msg = event.data as WSEventMap['message:new']
              if (msg?.channelId) {
                useMessageStore.getState().addMessage(msg)
                const activeId = useChannelStore.getState().activeChannelId
                if (msg.channelId !== activeId) {
                  const channelState = useChannelStore.getState()
                  const prefs = useSidebarPrefsStore.getState()
                  const isDM = channelState.dmChannels.some((c) => c.id === msg.channelId)
                  if (isDM && prefs.hiddenDMIds.has(msg.channelId)) {
                    prefs.unhideDM(msg.channelId)
                  }
                  useChannelStore.getState().incrementUnread(msg.channelId)
                  updateTitleUnread()
                  // Check notification preference
                  const currentUser = useUserStore.getState().user?.name
                  const isMentioned = currentUser && msg.content.includes(`@${currentUser}`)
                  const pref = localStorage.getItem(storageKey('notify')) || 'mentions'
                  if (pref === 'all' || (pref === 'mentions' && isMentioned)) {
                    sendBrowserNotification(msg)
                  }
                }
                // Update thread inbox if message is in a thread channel
                import('@/stores/threadInboxStore').then(({ useThreadInboxStore }) => {
                  const threadState = useThreadInboxStore.getState()
                  const matchingThread = threadState.threads.find((t) => t.id === msg.channelId)
                  if (matchingThread) {
                    threadState.updateThread(msg.channelId, {
                      lastActivityAt: msg.createdAt,
                      replyCount: matchingThread.replyCount + 1,
                    })
                  }
                })
              }
              break
            }
            case 'message:update': {
              const msg = event.data as WSEventMap['message:update']
              if (msg?.channelId) {
                useMessageStore.getState().updateMessage(msg)
              }
              break
            }
            case 'agent:status': {
              const d = event.data as WSEventMap['agent:status']
              if (d.agentId) useAgentStore.getState().updateStatus(d.agentId, d.status, d.errorDetail)
              break
            }
            case 'agent:activity': {
              const d = event.data as WSEventMap['agent:activity']
              if (d.agentId) {
                useAgentStore.getState().updateActivity(
                  d.agentId,
                  d.activity,
                  d.detail,
                  d.trajectory,
                  {
                    lastInputTokens: d.lastInputTokens,
                    totalOutputTokens: d.totalOutputTokens,
                    contextWindow: d.contextWindow,
                    totalCostUSD: d.totalCostUSD,
                    turnCount: d.turnCount,
                  },
                  {
                    attentionState: d.attentionState,
                    focusTaskId: d.focusTaskId,
                    focusScope: d.focusScope,
                    focusSince: d.focusSince,
                    priorityClass: d.priorityClass,
                    preempted: d.preempted,
                  },
                )
                // Forward individual entries to the live accumulator
                if (d.entries) {
                  for (const entry of d.entries) {
                    useAgentStore.getState().appendEntry(d.agentId, entry)
                  }
                }
              }
              break
            }
            case 'machine:updated': {
              const d = event.data as WSEventMap['machine:updated']
              if (d.machineId) {
                useMachineStatusStore.getState().applyMachineUpdated(d)
              }
              break
            }
            case 'user:presence': {
              const d = event.data as WSEventMap['user:presence']
              if (d.userId) {
                if (d.online) {
                  usePresenceStore.getState().setOnline(d.userId)
                } else {
                  usePresenceStore.getState().setOffline(d.userId)
                }
              }
              break
            }
            case 'task:update': {
              const task = event.data as WSEventMap['task:update']
              if (task?.channelId) useTaskStore.getState().updateTask(task)
              break
            }
            case 'agent:turn': {
              const d = event.data as WSEventMap['agent:turn']
              if (d.agentId) {
                const turn: Turn = {
                  id: '',
                  agentId: d.agentId,
                  sessionId: d.sessionId,
                  turnNumber: d.turnNumber,
                  startedAt: new Date().toISOString(),
                  entries: d.entries,
                  inputTokens: d.inputTokens,
                  outputTokens: d.outputTokens,
                  costUsd: d.costUsd,
                  contextWindow: d.contextWindow,
                  channelName: d.channelName,
                  contextUsagePct: d.contextUsagePct,
                }
                useAgentStore.getState().finalizeTurn(d.agentId, turn)
              }
              break
            }
            case 'agent:stop:error': {
              const d = event.data as WSEventMap['agent:stop:error']
              toastError(d.error || 'Failed to stop agent')
              break
            }
            case 'thread:update': {
              const { id, done } = event.data as WSEventMap['thread:update']
              import('@/stores/threadInboxStore').then(({ useThreadInboxStore }) => {
                useThreadInboxStore.getState().updateThread(id, { done })
              })
              break
            }
            case 'thread:activity': {
              const { threadId, replyCount, lastReply } = event.data as WSEventMap['thread:activity']
              import('@/stores/threadInboxStore').then(({ useThreadInboxStore }) => {
                useThreadInboxStore.getState().updateThread(threadId, {
                  replyCount,
                  lastActivityAt: lastReply.timestamp,
                })
              })
              break
            }
            case 'channel:updated': {
              const d = event.data as WSEventMap['channel:updated']
              if (d?.id) useChannelStore.getState().applyChannelUpdate(d)
              break
            }
            case 'prefs:updated': {
              const d = event.data as WSEventMap['prefs:updated']
              if (d?.prefs) applyPrefsFromServer(d.prefs)
              break
            }
          }

          // Route agent events to DevTools store
          const devToolsState = useDevToolsStore.getState()
          if (devToolsState.isSubscribed) {
            const devToolsEventTypes = [
              'agent:status', 'agent:activity', 'agent:turn',
              'agent:session', 'agent:session:end', 'agent:session:idle',
              'agent:prompt:info', 'agent:deliver:ack',
            ]
            if (devToolsEventTypes.includes(event.type)) {
              const data = event.data as Record<string, unknown>
              devToolsState.pushEvent({
                id: crypto.randomUUID(),
                timestamp: Date.now(),
                type: event.type,
                agentId: (data.agentId as string) || '',
                channelName: data.channelName as string | undefined,
                data,
              })
            }
          }
        } catch (err) {
          console.warn('[ws] failed to parse message:', err)
        }
      }

      ws.onclose = () => {
        if (!mounted) return
        useWSStore.getState().setStatus('disconnected')
        attemptRef.current++
        const backoff = Math.min(MIN_BACKOFF * Math.pow(2, attemptRef.current - 1), MAX_BACKOFF)
        timer = setTimeout(connect, backoff)
      }

      ws.onerror = () => {
        ws.close()
      }
    }

    connect()

    // Reconnect when active zone changes
    let prevZoneId = useZoneStore.getState().activeZoneId
    const unsubZone = useZoneStore.subscribe((state) => {
      if (state.activeZoneId !== prevZoneId && mounted) {
        prevZoneId = state.activeZoneId
        attemptRef.current = 0
        clearTimeout(timer)
        wsRef.current?.close()
        connect()
      }
    })

    return () => {
      mounted = false
      clearTimeout(timer)
      unsubZone()
      wsRef.current?.close()
    }
  }, [])
}

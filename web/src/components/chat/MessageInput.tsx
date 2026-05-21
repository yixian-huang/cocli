// Extend Window for Speech API
declare global {
  interface SpeechRecognition extends EventTarget {
    continuous: boolean
    interimResults: boolean
    lang: string
    start(): void
    stop(): void
    onresult: ((event: SpeechRecognitionEvent) => void) | null
    onerror: ((event: Event) => void) | null
    onend: (() => void) | null
  }
  interface SpeechRecognitionEvent extends Event {
    resultIndex: number
    results: SpeechRecognitionResultList
  }
  interface Window {
    SpeechRecognition: new () => SpeechRecognition
    webkitSpeechRecognition: new () => SpeechRecognition
  }
}

import { useState, useCallback, useRef, useEffect, type KeyboardEvent, type DragEvent } from 'react'
import { useMessageStore } from '@/stores/messageStore'
import { useChannelStore } from '@/stores/channelStore'
import { useViewStore } from '@/stores/viewStore'
import { attachments } from '@/api/client'
import { toastError } from '@/stores/toastStore'
import { MentionPopup, useMentionCandidates } from './MentionPopup'
import { Paperclip, X, Loader2, Quote, Mic, MicOff } from 'lucide-react'
import { storageKey } from '@/brand'
import { useTranslation } from 'react-i18next'

interface PendingFile {
  file: File
  uploading: boolean
  url?: string
}

// R2 (2026-04-25): the sender-side urgency picker was removed. Server-side
// rule classifier now determines priority alone — see
// docs/superpowers/specs/2026-04-25-platform-architectural-diagnosis.md §R2.

export function MessageInput({ channelId }: { channelId?: string }) {
  const { t } = useTranslation()
  const [text, setText] = useState('')
  const [sending, setSending] = useState(false)
  const [files, setFiles] = useState<PendingFile[]>([])
  const [dragOver, setDragOver] = useState(false)
  const [mentionQuery, setMentionQuery] = useState('')
  const [mentionStart, setMentionStart] = useState(-1)
  const [mentionIdx, setMentionIdx] = useState(0)
  const storeId = useChannelStore((s) => s.activeChannelId)
  const activeId = channelId ?? storeId
  const sendMessage = useMessageStore((s) => s.sendMessage)
  const quotedMessage = useViewStore((s) => s.quotedMessage)
  const setQuotedMessage = useViewStore((s) => s.setQuotedMessage)
  const fileInputRef = useRef<HTMLInputElement>(null)
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const mentionCandidates = useMentionCandidates(mentionQuery)

  // Voice-to-text
  const [recording, setRecording] = useState(false)
  const recognitionRef = useRef<SpeechRecognition | null>(null)
  const speechSupported = typeof window !== 'undefined' && ('SpeechRecognition' in window || 'webkitSpeechRecognition' in window)

  const toggleVoice = useCallback(() => {
    if (recording) {
      recognitionRef.current?.stop()
      setRecording(false)
      return
    }
    if (!speechSupported) return
    const SpeechRecognition = window.SpeechRecognition || window.webkitSpeechRecognition
    const recognition = new SpeechRecognition()
    recognition.continuous = true
    recognition.interimResults = true
    recognition.lang = 'zh-CN'
    let finalTranscript = ''
    recognition.onresult = (event: SpeechRecognitionEvent) => {
      let interim = ''
      for (let i = event.resultIndex; i < event.results.length; i++) {
        if (event.results[i].isFinal) {
          finalTranscript += event.results[i][0].transcript
        } else {
          interim += event.results[i][0].transcript
        }
      }
      setText((prev) => {
        const base = prev.endsWith(finalTranscript) ? prev : prev + finalTranscript
        return interim ? base + interim : base
      })
    }
    recognition.onerror = () => setRecording(false)
    recognition.onend = () => setRecording(false)
    recognitionRef.current = recognition
    recognition.start()
    setRecording(true)
  }, [recording, speechSupported])

  // Load draft from localStorage when channel changes
  useEffect(() => {
    if (activeId) {
      const saved = localStorage.getItem(storageKey(`draft-${activeId}`))
      setText(saved || '')
      requestAnimationFrame(() => {
        const el = textareaRef.current
        if (el) {
          el.style.height = 'auto'
          if (saved) el.style.height = Math.min(el.scrollHeight, 200) + 'px'
        }
      })
    }
  }, [activeId])

  // Debounced draft persistence to avoid synchronous localStorage writes on each key stroke.
  useEffect(() => {
    if (!activeId) return
    const timer = setTimeout(() => {
      if (text) localStorage.setItem(storageKey(`draft-${activeId}`), text)
      else localStorage.removeItem(storageKey(`draft-${activeId}`))
    }, 400)
    return () => clearTimeout(timer)
  }, [activeId, text])

  const uploadFile = useCallback(async (file: File) => {
    const entry: PendingFile = { file, uploading: true }
    setFiles((prev) => [...prev, entry])
    try {
      const result = await attachments.upload(file)
      setFiles((prev) =>
        prev.map((f) => (f.file === file ? { ...f, uploading: false, url: result.url } : f)),
      )
    } catch (err) {
      toastError(err instanceof Error ? err.message : 'Upload failed')
      setFiles((prev) => prev.filter((f) => f.file !== file))
    }
  }, [])

  const handleFiles = useCallback(
    (fileList: FileList) => {
      Array.from(fileList).forEach(uploadFile)
    },
    [uploadFile],
  )

  const removeFile = (file: File) => {
    setFiles((prev) => prev.filter((f) => f.file !== file))
  }

  const handleSend = useCallback(async () => {
    const fileLinks = files
      .filter((f) => f.url)
      .map((f) => `[${f.file.name}](${f.url})`)
      .join('\n')
    const quoteBlock = quotedMessage
      ? `> **${quotedMessage.senderName}:** ${quotedMessage.content.split('\n')[0].slice(0, 120)}\n\n`
      : ''
    const content = [quoteBlock + text.trim(), fileLinks].filter(Boolean).join('\n\n')
    if (!content || !activeId || sending) return
    setSending(true)
    try {
      await sendMessage(activeId, content)
      setText('')
      setFiles([])
      setQuotedMessage(null)
      localStorage.removeItem(storageKey(`draft-${activeId}`))
    } catch {
      toastError('Failed to send message')
    } finally {
      setSending(false)
    }
  }, [text, files, quotedMessage, activeId, sending, sendMessage, setQuotedMessage])

  const handleTextChange = (value: string) => {
    setText(value)
    const el = textareaRef.current
    if (!el) { setMentionQuery(''); setMentionStart(-1); return }
    const pos = el.selectionStart
    const before = value.slice(0, pos)
    const match = before.match(/@(\w*)$/)
    if (match) {
      setMentionQuery(match[1])
      setMentionStart(pos - match[0].length)
      setMentionIdx(0)
    } else {
      setMentionQuery('')
      setMentionStart(-1)
    }
  }

  const insertMention = (name: string) => {
    if (mentionStart < 0) return
    const el = textareaRef.current
    const pos = el?.selectionStart ?? text.length
    const before = text.slice(0, mentionStart)
    const after = text.slice(pos)
    const newText = `${before}@${name} ${after}`
    setText(newText)
    setMentionQuery('')
    setMentionStart(-1)
    setTimeout(() => {
      const newPos = mentionStart + name.length + 2
      el?.setSelectionRange(newPos, newPos)
      el?.focus()
    }, 0)
  }

  const showMention = mentionStart >= 0 && mentionCandidates.length > 0

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (showMention) {
      if (e.key === 'ArrowDown') {
        e.preventDefault()
        setMentionIdx((i) => Math.min(i + 1, mentionCandidates.length - 1))
        return
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault()
        setMentionIdx((i) => Math.max(i - 1, 0))
        return
      }
      if (e.key === 'Enter' || e.key === 'Tab') {
        e.preventDefault()
        insertMention(mentionCandidates[mentionIdx].name)
        return
      }
      if (e.key === 'Escape') {
        e.preventDefault()
        setMentionQuery('')
        setMentionStart(-1)
        return
      }
    }
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      handleSend()
    }
  }

  const handleDrop = (e: DragEvent) => {
    e.preventDefault()
    setDragOver(false)
    if (e.dataTransfer.files.length > 0) {
      handleFiles(e.dataTransfer.files)
    }
  }

  const hasContent = text.trim() || files.some((f) => f.url)
  const anyUploading = files.some((f) => f.uploading)

  if (!activeId) return null

  return (
    <div
      className={`shrink-0 border-t border-border-default bg-surface-secondary px-3 pt-2 md:px-4 ${dragOver ? 'bg-accent-secondary' : ''}`}
      style={{ paddingBottom: 'max(0.75rem, env(safe-area-inset-bottom, 0.75rem))' }}
      onDragOver={(e) => { e.preventDefault(); setDragOver(true) }}
      onDragLeave={() => setDragOver(false)}
      onDrop={handleDrop}
    >
      <div
        className="relative bg-surface-panel border border-border-default rounded-[var(--radius-base)] transition-shadow focus-within:shadow-[0_0_0_2px_var(--accent-signature)] focus-within:border-transparent"
        style={{ transitionDuration: 'var(--motion-fast)', transitionTimingFunction: 'var(--ease-out)' }}
      >
        {quotedMessage && (
          <div className="flex items-center gap-2 border-b border-border-default px-3 pt-2 pb-1 text-xs">
            <Quote className="h-3 w-3 text-primary shrink-0" />
            <span className="text-muted-foreground">Quoting</span>
            <span className="font-medium">{quotedMessage.senderName}</span>
            <span className="flex-1 truncate text-muted-foreground">{quotedMessage.content.split('\n')[0].slice(0, 80)}</span>
            <button
              onClick={() => setQuotedMessage(null)}
              className="p-0.5 rounded hover:bg-accent text-muted-foreground"
            >
              <X className="h-3 w-3" />
            </button>
          </div>
        )}
        {showMention && (
          <MentionPopup query={mentionQuery} selectedIndex={mentionIdx} onSelect={insertMention} />
        )}
        {files.length > 0 && (
          <div className="flex flex-wrap gap-2 px-3 pt-3">
            {files.map((f, i) => (
              <div key={i} className="flex items-center gap-1.5 border border-border-default bg-surface-tertiary px-2.5 py-1 text-xs text-content-secondary">
                {f.uploading ? (
                  <Loader2 className="h-3 w-3 animate-spin text-primary" />
                ) : (
                  <Paperclip className="h-3 w-3 text-muted-foreground" />
                )}
                <span className="truncate max-w-[120px]">{f.file.name}</span>
                <button onClick={() => removeFile(f.file)} className="hover:text-destructive transition-colors">
                  <X className="h-3 w-3" />
                </button>
              </div>
            ))}
          </div>
        )}
        <div className="flex items-end gap-1">
          <input
            ref={fileInputRef}
            type="file"
            multiple
            className="hidden"
            onChange={(e) => e.target.files && handleFiles(e.target.files)}
          />
          <button
            onClick={() => fileInputRef.current?.click()}
            className="h-10 w-10 flex items-center justify-center hover:bg-accent text-muted-foreground hover:text-foreground shrink-0 transition-colors"
            title="Attach file"
          >
            <Paperclip className="h-4 w-4" />
          </button>
          {speechSupported && (
            <button
              onClick={toggleVoice}
              className={`shrink-0 flex items-center justify-center rounded-md transition-all ${
                recording
                  ? 'h-10 w-10 sm:h-10 sm:w-10 bg-error text-destructive-foreground animate-pulse shadow-whisper'
                  : 'h-10 w-10 sm:h-10 sm:w-10 text-muted-foreground hover:bg-accent hover:text-foreground'
              }`}
              title={recording ? t('workspace.message.voiceStop') : t('workspace.message.voiceStart')}
            >
              {recording ? <MicOff className="h-5 w-5 sm:h-4 sm:w-4" /> : <Mic className="h-5 w-5 sm:h-4 sm:w-4" />}
            </button>
          )}
          <textarea
            ref={textareaRef}
            data-message-input
            value={text}
            onChange={(e) => handleTextChange(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={t('workspace.message.placeholder')}
            rows={3}
            className="flex-1 resize-none bg-transparent text-content-primary px-1 py-2.5 text-base focus:outline-none min-h-[72px] max-h-[200px] placeholder:font-signal placeholder:text-content-muted placeholder:lowercase placeholder:text-xs"
            style={{ height: 'auto', overflow: 'hidden' }}
            onInput={(e) => {
              const target = e.target as HTMLTextAreaElement
              target.style.height = 'auto'
              target.style.height = Math.min(target.scrollHeight, 200) + 'px'
            }}
          />
          <button
            type="button"
            onClick={handleSend}
            disabled={!hasContent || sending || anyUploading}
            className="bg-accent-signature text-accent-on-signature font-signal font-bold uppercase tracking-[0.08em] text-xs px-3.5 py-1.5 rounded-[var(--radius-base)] hover:bg-accent-signature-hover shrink-0 disabled:opacity-50 disabled:pointer-events-none transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            style={{ transitionDuration: 'var(--motion-fast)', transitionTimingFunction: 'var(--ease-out)' }}
          >
            {sending ? <Loader2 className="h-4 w-4 animate-spin" /> : t('workspace.chat.composer.send', 'Send')}
          </button>
        </div>
      </div>
    </div>
  )
}

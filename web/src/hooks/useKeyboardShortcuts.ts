import { useEffect } from 'react'

export interface ShortcutDefinition {
  key: string
  handler: (event: KeyboardEvent) => void
  enabled?: boolean
  priority?: number
  preventDefault?: boolean
  stopPropagation?: boolean
  allowInInput?: boolean
  mod?: boolean
  shift?: boolean
  alt?: boolean
  ctrl?: boolean
  meta?: boolean
}

interface RegisteredShortcut extends ShortcutDefinition {
  token: symbol
  order: number
}

let nextOrder = 0
let registeredShortcuts: RegisteredShortcut[] = []
let listenerAttached = false
let handleKeydownRef: ((event: KeyboardEvent) => void) | null = null

function normalizeKey(key: string) {
  return key.toLowerCase()
}

function isEditableTarget(target: EventTarget | null) {
  if (!(target instanceof HTMLElement)) return false
  const tagName = target.tagName
  return (
    tagName === 'INPUT'
    || tagName === 'TEXTAREA'
    || tagName === 'SELECT'
    || target.isContentEditable
    || target.getAttribute('role') === 'textbox'
  )
}

function matchesShortcut(shortcut: RegisteredShortcut, event: KeyboardEvent) {
  if (shortcut.enabled === false) return false
  if (!shortcut.allowInInput && isEditableTarget(event.target)) return false
  if (normalizeKey(event.key) !== normalizeKey(shortcut.key)) return false

  if (shortcut.mod !== undefined && (event.metaKey || event.ctrlKey) !== shortcut.mod) return false
  if (shortcut.shift !== undefined && event.shiftKey !== shortcut.shift) return false
  if (shortcut.alt !== undefined && event.altKey !== shortcut.alt) return false
  if (shortcut.ctrl !== undefined && event.ctrlKey !== shortcut.ctrl) return false
  if (shortcut.meta !== undefined && event.metaKey !== shortcut.meta) return false

  return true
}

function sortedShortcuts() {
  return [...registeredShortcuts].sort((a, b) => {
    const priorityDiff = (b.priority ?? 0) - (a.priority ?? 0)
    if (priorityDiff !== 0) return priorityDiff
    return b.order - a.order
  })
}

function ensureListener() {
  if (listenerAttached || typeof window === 'undefined') return

  handleKeydownRef = (event: KeyboardEvent) => {
    for (const shortcut of sortedShortcuts()) {
      if (!matchesShortcut(shortcut, event)) continue

      if (shortcut.preventDefault !== false) {
        event.preventDefault()
      }
      if (shortcut.stopPropagation) {
        event.stopPropagation()
      }

      shortcut.handler(event)
      return
    }
  }

  window.addEventListener('keydown', handleKeydownRef)
  listenerAttached = true
}

function cleanupListener() {
  if (!listenerAttached || typeof window === 'undefined' || !handleKeydownRef) return
  window.removeEventListener('keydown', handleKeydownRef)
  handleKeydownRef = null
  listenerAttached = false
}

export function useKeyboardShortcuts(shortcuts: ShortcutDefinition[]) {
  useEffect(() => {
    if (shortcuts.length === 0) return

    const token = Symbol('keyboard-shortcuts')
    registeredShortcuts = [
      ...registeredShortcuts,
      ...shortcuts.map((shortcut) => ({
        ...shortcut,
        token,
        order: nextOrder++,
      })),
    ]

    ensureListener()

    return () => {
      registeredShortcuts = registeredShortcuts.filter((shortcut) => shortcut.token !== token)
      if (registeredShortcuts.length === 0) {
        cleanupListener()
      }
    }
  }, [shortcuts])
}

export function resetKeyboardShortcutsForTests() {
  registeredShortcuts = []
  nextOrder = 0
  cleanupListener()
}

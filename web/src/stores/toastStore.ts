import { create } from 'zustand'

export const MAX_TOASTS = 4
export const TOAST_ENTER_DELAY_MS = 10
export const TOAST_EXIT_MS = 200
export const TOAST_DURATIONS = {
  success: 3000,
  info: 3000,
  warn: 6000,
  error: null,
  critical: null,
} as const

export type ToastType = keyof typeof TOAST_DURATIONS
export type ToastPhase = 'entering' | 'visible' | 'closing'

export interface Toast {
  id: number
  message: string
  type: ToastType
  phase: ToastPhase
  durationMs: number | null
}

let nextId = 0
const autoDismissTimers = new Map<number, ReturnType<typeof setTimeout>>()
const phaseTimers = new Map<number, ReturnType<typeof setTimeout>>()

function clearTimer(map: Map<number, ReturnType<typeof setTimeout>>, id: number) {
  const timer = map.get(id)
  if (!timer) return
  clearTimeout(timer)
  map.delete(id)
}

function clearToastTimers(id: number) {
  clearTimer(autoDismissTimers, id)
  clearTimer(phaseTimers, id)
}

function clearAllToastTimers() {
  for (const timer of autoDismissTimers.values()) {
    clearTimeout(timer)
  }
  autoDismissTimers.clear()

  for (const timer of phaseTimers.values()) {
    clearTimeout(timer)
  }
  phaseTimers.clear()
}

function markToastVisible(id: number) {
  useToastStore.setState((state) => ({
    toasts: state.toasts.map((toast) =>
      toast.id === id && toast.phase === 'entering'
        ? { ...toast, phase: 'visible' }
        : toast,
    ),
  }))
}

function removeToastNow(id: number) {
  clearToastTimers(id)
  useToastStore.setState((state) => ({
    toasts: state.toasts.filter((toast) => toast.id !== id),
  }))
}

interface ToastState {
  toasts: Toast[]
  addToast: (message: string, type?: ToastType) => number
  dismissToast: (id: number) => void
  dismissLatestToast: () => void
  clearToasts: () => void
}

export const useToastStore = create<ToastState>((set, get) => ({
  toasts: [],

  addToast: (message, type = 'info') => {
    const id = ++nextId
    const durationMs = TOAST_DURATIONS[type]
    const toast: Toast = {
      id,
      message,
      type,
      phase: 'entering',
      durationMs,
    }

    const existing = get().toasts
    const nextToasts = [toast, ...existing].slice(0, MAX_TOASTS)
    const dropped = existing.filter((entry) => !nextToasts.some((next) => next.id === entry.id))

    dropped.forEach((entry) => clearToastTimers(entry.id))
    set({ toasts: nextToasts })

    const enterTimer = setTimeout(() => {
      phaseTimers.delete(id)
      markToastVisible(id)
    }, TOAST_ENTER_DELAY_MS)
    phaseTimers.set(id, enterTimer)

    if (durationMs !== null) {
      const autoTimer = setTimeout(() => {
        autoDismissTimers.delete(id)
        get().dismissToast(id)
      }, durationMs)
      autoDismissTimers.set(id, autoTimer)
    }

    return id
  },

  dismissToast: (id) => {
    const toast = get().toasts.find((entry) => entry.id === id)
    if (!toast || toast.phase === 'closing') return

    clearTimer(autoDismissTimers, id)
    set((state) => ({
      toasts: state.toasts.map((entry) =>
        entry.id === id ? { ...entry, phase: 'closing' } : entry,
      ),
    }))

    const exitTimer = setTimeout(() => {
      phaseTimers.delete(id)
      removeToastNow(id)
    }, TOAST_EXIT_MS)
    phaseTimers.set(id, exitTimer)
  },

  dismissLatestToast: () => {
    const latestToast = get().toasts[0]
    if (!latestToast) return
    get().dismissToast(latestToast.id)
  },

  clearToasts: () => {
    clearAllToastTimers()
    set({ toasts: [] })
  },
}))

export function toast(message: string, type: ToastType = 'info') {
  return useToastStore.getState().addToast(message, type)
}

export function toastError(message: string) {
  return useToastStore.getState().addToast(message, 'error')
}

export function toastWarn(message: string) {
  return useToastStore.getState().addToast(message, 'warn')
}

export function toastCritical(message: string) {
  return useToastStore.getState().addToast(message, 'critical')
}

export function dismissLatestToast() {
  useToastStore.getState().dismissLatestToast()
}

export function resetToastStore() {
  clearAllToastTimers()
  nextId = 0
  useToastStore.setState({ toasts: [] })
}

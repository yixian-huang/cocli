import { create } from 'zustand'
import type { Zone } from '@/lib/types'
import * as api from '@/api/client'
import { storageKey } from '@/brand'

const ZONE_ID_STORAGE_KEY = storageKey('active-zone')
const ZONE_SLUG_STORAGE_KEY = storageKey('active-zone-slug')

function readStoredZoneId(): string | null {
  try {
    if (typeof localStorage !== 'undefined' && typeof localStorage.getItem === 'function') {
      return localStorage.getItem(ZONE_ID_STORAGE_KEY)
    }
  } catch {
    // ignore
  }
  return null
}

function persistZoneId(zoneId: string) {
  try {
    if (typeof localStorage !== 'undefined' && typeof localStorage.setItem === 'function') {
      localStorage.setItem(ZONE_ID_STORAGE_KEY, zoneId)
    }
  } catch {
    // ignore
  }
}

function readStoredZoneSlug(): string | null {
  try {
    if (typeof localStorage !== 'undefined' && typeof localStorage.getItem === 'function') {
      return localStorage.getItem(ZONE_SLUG_STORAGE_KEY)
    }
  } catch {
    // ignore
  }
  return null
}

function persistZoneSlug(slug: string) {
  try {
    if (typeof localStorage !== 'undefined' && typeof localStorage.setItem === 'function') {
      localStorage.setItem(ZONE_SLUG_STORAGE_KEY, slug)
    }
  } catch {
    // ignore
  }
}

interface ZoneState {
  zones: Zone[]
  activeZoneId: string | null
  activeZoneSlug: string | null
  activeZoneThemeId: string | null
  loading: boolean
  fetchZones: () => Promise<void>
  setActiveZone: (zoneId: string) => void
  setActiveZoneSlug: (slug: string) => void
  createZone: (name: string, slug: string) => Promise<Zone>
}

export const useZoneStore = create<ZoneState>((set, get) => ({
  zones: [],
  activeZoneId: readStoredZoneId(),
  activeZoneSlug: readStoredZoneSlug(),
  activeZoneThemeId: null,
  loading: true,

  fetchZones: async () => {
    try {
      const zones = await api.zones.list()
      const currentActive = get().activeZoneId
      const currentSlug = get().activeZoneSlug

      const activeZone =
        (currentSlug ? zones.find(z => z.slug === currentSlug) : null) ||
        (currentActive ? zones.find(z => z.id === currentActive) : null) ||
        zones[0] ||
        null

      const activeZoneId = activeZone?.id || null
      const activeZoneSlug = activeZone?.slug || null

      if (activeZoneId) persistZoneId(activeZoneId)
      if (activeZoneSlug) persistZoneSlug(activeZoneSlug)

      set({
        zones,
        activeZoneId,
        activeZoneSlug,
        activeZoneThemeId: activeZone?.themeId ?? null,
        loading: false,
      })
    } catch {
      set({ loading: false })
    }
  },

  setActiveZone: (zoneId: string) => {
    persistZoneId(zoneId)
    const zone = get().zones.find(z => z.id === zoneId)
    if (zone?.slug) persistZoneSlug(zone.slug)
    set({
      activeZoneId: zoneId,
      activeZoneSlug: zone?.slug || null,
      activeZoneThemeId: zone?.themeId ?? null,
    })
  },

  setActiveZoneSlug: (slug: string) => {
    persistZoneSlug(slug)
    const zone = get().zones.find(z => z.slug === slug)
    if (zone?.id) persistZoneId(zone.id)
    set({
      activeZoneSlug: slug,
      activeZoneId: zone?.id || get().activeZoneId,
      activeZoneThemeId: zone?.themeId ?? null,
    })
  },

  createZone: async (name: string, slug: string) => {
    const zone = await api.zones.create(name, slug)
    set((s) => ({ zones: [...s.zones, zone] }))
    return zone
  },
}))

// Single source of truth for brand-coupled values on the web side.
// A future rename touches this file plus internal/brand/brand.go and
// scripts/brand.sh; everything else composes from BRAND.
//
// Keep this file dependency-free so web-ops (sibling workspace) can copy
// or symlink it later without dragging in app code.

export const BRAND = {
  /** User-visible product name. Shown in <title>, headings, signup pages. */
  displayName: 'Cocli',

  /** Lowercase identifier — base of storage keys, paths, prefixes. */
  slug: 'cocli',

  /** Canonical second-level domain. */
  rootDomain: 'cocli.ai',

  /** Prefix used for every browser localStorage key the app writes. */
  storagePrefix: 'cocli',

  /** Prefix on API keys minted server-side. Exposed here only for display. */
  apiKeyPrefix: 'cocli_',

  /** Canonical public-facing repository URL. */
  githubUrl: 'https://github.com/yixian-huang/cocli',
} as const

/**
 * Compose a localStorage key from the brand prefix plus a stable suffix.
 * Example: `storageKey('active-zone')` -> `'cocli-active-zone'`.
 */
export const storageKey = (suffix: string): string =>
  `${BRAND.storagePrefix}-${suffix}`

/**
 * Default `<title>` text. Stores that decorate it with unread counts read
 * this instead of a hard-coded string.
 */
export const defaultTitle = BRAND.displayName

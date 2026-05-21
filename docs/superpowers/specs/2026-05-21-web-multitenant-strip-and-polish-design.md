---
title: web/ multi-tenant strip + first-run wizard + plugin manager mockup + branding + ESLint cleanup
date: 2026-05-21
status: draft
scope: web/ + shared/ (frontend slice of cocli OSS M0.0.1)
parent_spec: ~/code/1HzAi/docs/superpowers/specs/2026-05-21-cocli-oss-launch-design.md (§6.1, §4.1, §9.3)
worktree: .claude/worktrees/web-multitenant-strip-and-polish (branch `worktree-web-multitenant-strip-and-polish`)
---

# Design — web/ multi-tenant strip + first-run wizard + plugin manager mockup + branding + ESLint cleanup

## 0. Context

M0 of cocli OSS shipped 2026-05-21 (workspace skeleton + cherry-picked
`web/` + `shared/` from `1HzAi/web`). The cherry-pick brought the full SaaS
multi-tenant frontend wholesale: zones, auth, skills, cron, daemon-manager,
provider-credentials, push-tokens. None of it belongs in cocli local v0
(single-tenant, single-user, single-binary, single-machine).

This slice prepares the frontend for M0.0.1+ backend work:

1. Delete every multi-tenant artefact (delete + flatten + shim, never hide
   behind flags).
2. Add a 3-step first-run wizard scaffold (locate Claude → create first
   agent → guide-to-#general) — stores-only, no backend dependency.
3. Add a full-CRUD plugin manager UI mockup at `/settings/plugins` —
   stores-only, mimics the spec §4.1 contract (`GET/POST/DELETE
   /api/plugins`) including the "token shown once" reveal flow.
4. Replace branding to `cocli local` wordmark + favicon + page title.
5. Drive ESLint to zero errors (residue after strip: ~16 in test files).

**Why now**: M0.0.1 starts the Rust backend (channels + messages). Frontend
needs to be ready to wire against the real API the moment it lands. Without
this slice the frontend would call SaaS URLs that don't exist and crash on
load.

**Why this scope**: Spec §6.1 names this exact slice ("~1-1.5 周"). The work
is concurrent with backend M0.0.1; a separate session owns `crates/` + `bin/`
+ `scripts/`. This session never touches those paths.

## 1. Goals + non-goals

### 1.1 Goals

- After this slice, `npm run dev` launches the app, the first-run wizard
  overlays, all three steps click through, and the user lands in
  `/channel/general` (rendered against in-memory stores).
- `/settings/plugins` page renders, register dialog works, token reveal
  modal shows a UUID, plugins persist in `localStorage`, revoke works.
- `npm run lint` exits 0.
- `npm run build` produces a dist; `vite preview` serves a working SPA
  (no backend required — all data is in-memory).
- All existing passing tests still pass; new tests cover the wizard and
  plugin manager golden paths.
- The shape of `shared/api/client.ts` is aligned with spec §4.1 so that
  M0.0.1 backend can implement the contract by reading TypeScript.

### 1.2 Non-goals

- No real backend integration. `vite dev` proxy → `:8080` is intentionally
  broken until M0.0.1 binds an actual server. The app must work without
  any HTTP traffic, except `/api/version` + `/api/health` which may be
  silently retried.
- No MSW or network-mock infrastructure. Stores hold the data.
- No new branding palette. Reuse SaaS colors (spec §9.3 explicitly says
  v0 is extremely minimal — wordmark + favicon + title only).
- No new product features beyond the three named slices. No refactors
  outside this scope (e.g. don't restructure component hierarchy, don't
  upgrade dependencies, don't change build tooling).
- No upstream-cherry-pick friendliness. Spec §3.3 calls the cherry a
  one-shot snapshot; from here `web/` is its own thing. Delete freely,
  ignore merge-conflict cost from a future re-cherry.
- No CLI `--skip-wizard --claude-path=... --auto-create-agent` flags —
  those need backend cooperation. Honor only the URL `?skip-wizard=1`
  fallback so screenshots / CI smoke can bypass.

## 2. Strip strategy — delete + flatten + shim

Approach **A** (chosen over B "feature-flag hide" and C "rename
zone→workspace"):

- **A. Delete + flatten + shim** — physically remove zone/auth/skills/cron
  files; flatten `/z/:zoneSlug/*` router branch to root; replace
  `userStore` with a 30-LOC hardcoded single-user shim.
- **B (rejected)** — Hidden via `featureFlagStore`. Dead code bloats dist,
  noises lint, and "soft" deletions ALWAYS regrow.
- **C (rejected)** — Violates spec §6.1 ("delete zone switcher", "削平
  路径前缀").

Approved scope (final after user Q3):

- Delete `routes/daemons/`, `routes/wiki/`, `routes/devtools/` directories
  entirely. No empty stub. Future inspector / dev-tools belongs to a later
  milestone — design then.

### 2.1 Delete list (wholesale, ~50 files)

**Components**:
- `web/src/components/zone/` — entire directory (11 files: SkillsLibrary*,
  ZoneSettingsView*)
- `web/src/components/sidebar/{ZoneSwitcher,ZoneMembersPanel,ZoneThemeSelect,
  CreateZoneDialog,UserList,ProviderKeysTab,CreateKeyDialog,AddDaemonDialog,
  InviteLinks}.tsx` (+ co-located `.test.tsx`)
- `web/src/components/{LoginPage,InviteSignup,UserProfile}.tsx`
- `web/src/components/agents/SkillsTab*` (Skills UI per spec §6.1 "delete")

**Routes**:
- `web/src/routes/{LoginRoute,InviteRoute,ZoneDevToolsRoute,ZonePanelRoute,
  DaemonDetailRoute,LegacyDevtoolsRedirect}.tsx`
- `web/src/routes/{zone,daemons,wiki,devtools}/` — entire dirs if present

**Stores**:
- `web/src/stores/{zoneStore,zoneAdminStore,zoneTaskBoardStore,
  chatrsCredentialsStore}.ts` (+ tests)

### 2.2 Surgical-edit list (~25 files)

- `web/src/router.tsx` — flatten:
  - Remove the entire `/z/:zoneSlug` route branch and every nested
    `history | tasks | members | wiki | keys | devtools | daemons |
    daemons/:machineId` child (all are deleted features).
  - Remove the `/login` and `/invite/:code` paths.
  - Keep at root: `/`, `/channel/:channelId`, `/channel/:channelId/msg/:id`,
    `/agent/:id`.
  - Add at root: `/settings/plugins`.
  - Add `path: '*'` catchall → `<Navigate to="/" replace />` (covers stale
    bookmarks; see §9.4).
- `web/src/stores/userStore.ts` — rewrite to ~30 LOC:
  ```ts
  // single hardcoded user, no network, no init
  export const useUserStore = create<UserState>(() => ({
    user: { id: 'local', name: 'owner', displayName: 'owner' },
    init: () => {/* no-op */},
  }))
  ```
- `web/src/stores/{channelStore,agentStore,messageStore,taskStore,
  threadInboxStore,threadStore,historyStore,...}.ts` — every `client.X.Y(
  zoneId, ...)` call becomes `client.X.Y(...)`. The `zoneId` parameter
  vanishes from the entire stack.
- `web/src/api/*` (web-side wrappers if any) — same de-zoning.
- `web/src/App.tsx` — drop ZoneSwitcher / Zone-aware sidebar branches;
  mount `<FirstRunWizard />` overlay; the navigation tree is now flat.
- `web/src/components/sidebar/SidebarTabs.tsx` — drop tabs that referenced
  deleted features (skills, members, credentials, daemons, zone-settings).
- `web/src/components/sidebar/AgentCreateForm.tsx`,
  `web/src/components/sidebar/AgentList.tsx` — strip zoneId arg.
- `shared/api/client.ts` — major rewrite per §7.
- `shared/types/index.ts` — major trim per §8.
- `shared/stores/tasks.ts` — replace `zoneTasks.list(zoneId,…)` with
  `tasks.list(channelId,…)` or drop module if the only consumer was
  `zoneTaskBoardStore`.
- `shared/stores/filter.ts` — drop comment ref to `zoneTasks.list`.

### 2.3 Replace / new (~7 files)

- `web/src/components/wizard/FirstRunWizard.tsx`
- `web/src/components/wizard/steps/{LocateClaude,CreateAgent,TryIt}Step.tsx`
- `web/src/stores/wizardStore.ts`
- `web/src/components/settings/plugins/PluginsPage.tsx`
- `web/src/components/settings/plugins/{RegisterPluginDialog,
  TokenRevealDialog,PluginRow,RevokeConfirmDialog}.tsx`
- `web/src/routes/SettingsPluginsRoute.tsx`
- `web/src/stores/pluginsStore.ts`

### 2.4 Net effect

- ~50 files removed
- ~25 files edited
- ~7 files added
- ~2000 LOC net reduction
- All `zoneId: string` arguments removed from public store and client
  signatures
- All `useUserStore` consumers continue to compile (shim preserves shape)

## 3. First-run wizard (Section 6.6 of parent spec)

### 3.1 Mount logic

`App.tsx` reads `localStorage['cocli-first-run-complete']` on first render.
If not `'true'`, mount `<FirstRunWizard />` as a full-screen overlay that
blocks app interaction. URL `?skip-wizard=1` bypasses (sets flag).

### 3.2 Store shape

```ts
// web/src/stores/wizardStore.ts (~60 LOC)
type Model = 'claude-sonnet-4-6' | 'claude-haiku-4-5' | 'claude-opus-4-7'

interface DraftAgent { name: string; model: Model }

interface WizardState {
  step: 1 | 2 | 3
  complete: boolean
  claudePath: string
  detectedAt: string | null
  draftAgent: DraftAgent
  init: () => void                // read localStorage
  next: () => void
  back: () => void
  setClaudePath: (p: string) => void
  detectClaudePath: () => Promise<void>  // stores-only: 600ms fake delay + success
  setDraftAgent: (a: Partial<DraftAgent>) => void
  finish: () => void              // persist flag + push agent into agentStore
}
```

`finish()` pushes the draft agent into `agentStore` via a new `commit()`
action that adds an in-memory agent (UUID id, status `'offline'`, fields
echoed). The agent then appears in the sidebar agent list.

`localStorage` keys (use `storageKey()` from `shared/brand`):
- `cocli-first-run-complete` — `'true'` once finished
- `cocli-wizard-state` — JSON of `{step, claudePath, draftAgent}` for
  resumption mid-wizard (refresh-survival)

### 3.3 Step components

All under `web/src/components/wizard/`.

**`FirstRunWizard.tsx`** — modal shell (centered card, 480px wide), header
"Welcome to cocli local", progress dots (1/2/3), body switches on
`store.step`, footer Back/Next buttons (Next enables when step is valid).

**`steps/LocateClaudeStep.tsx`** — body:
- Headline "Where is your Claude CLI?"
- Text input bound to `claudePath`, placeholder `/usr/local/bin/claude`
- "Detect" button → calls `detectClaudePath()` (600 ms spinner, succeeds
  unconditionally, sets `detectedAt`), shows green check on success
- Hint text: "Leave blank for now — we'll auto-detect when the binary is
  ready in M0.0.2"
- Validation: always passable (no required field in stores-only mode)

**`steps/CreateAgentStep.tsx`** — body:
- Headline "Create your first agent"
- Text input for `name` (placeholder `@assistant`, validation: non-empty,
  must start with `@` or auto-prepend, lowercase + `[a-z0-9-]+`)
- Model dropdown — three hardcoded options (sonnet 4.6 default, haiku 4.5,
  opus 4.7)
- Helper text: "This agent lives on your machine. You can change the model
  later."

**`steps/TryItStep.tsx`** — body:
- ✓ "You're all set!" headline
- Subhead "Go say hi in #general"
- Primary CTA "Open #general →" → `finish()` then `navigate('/channel/general')`
- Skip link "Maybe later" → `finish()` but stay on current page

### 3.4 Tests

Per-step `.test.tsx`:
- Renders without throwing
- Next button disabled when invalid; enabled when valid
- Click Next advances `store.step`
- Click Back decreases
- TryItStep CTA invokes `finish()` (mock router push asserted)

Plus `wizardStore.test.ts` for state transitions + localStorage round-trip.

## 4. Plugin manager mockup (spec §4.1 + §4.4)

### 4.1 Route + sidebar entry

- Route: `/settings/plugins` → `SettingsPluginsRoute`
- Sidebar gets a single new entry "Settings → Plugins" (no nested Settings
  hub yet — that's a future polish). For now a plain icon button at the
  bottom of the sidebar wired to `navigate('/settings/plugins')`.

### 4.2 Store shape

```ts
// web/src/stores/pluginsStore.ts (~80 LOC)
type Capability = 'inbound-bridge' | 'outbound-bridge'

interface Plugin {
  id: string
  name: string
  capabilities: Capability[]
  createdAt: string
  lastSeenAt: string | null  // always null in stores-only
}

interface PluginsState {
  plugins: Plugin[]
  init: () => void                                       // load from localStorage
  list: () => Promise<Plugin[]>
  register: (name: string, caps: Capability[]) => Promise<{
    plugin: Plugin
    token: string  // crypto.randomUUID() — shown once, NOT persisted in store
  }>
  revoke: (id: string) => Promise<void>
}
```

`localStorage` key: `cocli-plugins`. Token is **deliberately** not stored —
matches spec §4.4 invariant that token plaintext is only available at
registration time. If user dismisses TokenRevealDialog without copying,
they must revoke and re-register.

### 4.3 Components (`web/src/components/settings/plugins/`)

**`PluginsPage.tsx`** — top-level route content:
- Header "Plugins" + subhead "Bridge external services into your channels"
- Right-aligned primary button "Register plugin"
- Body: empty state OR list of `<PluginRow>` rows
- Empty state: centered illustration placeholder (`<div class="opacity-50">No
  plugins yet</div>`) + body text "Register one to bridge Telegram, Slack,
  Discord, or your own custom bridge into a cocli channel."

**`PluginRow.tsx`** — single row:
- Name (mono font)
- Capability badges (pill: "inbound-bridge" green, "outbound-bridge" blue)
- "Created 3 days ago" relative timestamp
- "Last seen: never" (always in stores-only)
- Trailing trash icon → `RevokeConfirmDialog`

**`RegisterPluginDialog.tsx`** — modal:
- Text input "Plugin name" (validation: 1-64 chars, lowercase + `[a-z0-9-]+`)
- Two checkboxes for capabilities (at least one required)
- Submit button "Register" → calls `pluginsStore.register(...)`, on success
  closes self + opens `TokenRevealDialog` with the returned plaintext token

**`TokenRevealDialog.tsx`** — modal:
- Header "Plugin registered"
- Body: large monospace box with the token + Copy-to-clipboard button
- Yellow warning text: "Save this token — it won't be shown again. If you
  lose it, revoke the plugin and register a new one."
- Primary button "I've saved it" → close

**`RevokeConfirmDialog.tsx`** — small destructive-confirm modal:
- Body "Revoke `<plugin-name>`? Connected bridges will disconnect."
- Cancel + Revoke (red) buttons

### 4.4 i18n

Add `web/src/i18n/locales/{en,zh}.json` keys under `plugins.*` and
`wizard.*` namespaces, matching the existing i18next pattern. Provide both
EN and ZH for the strings (~30 keys total). Existing translations are not
touched.

### 4.5 Tests

- `pluginsStore.test.ts` — register flow returns token, store has plugin,
  revoke removes it, localStorage round-trips
- `PluginsPage.test.tsx` — empty state + populated state render
- `RegisterPluginDialog.test.tsx` — validation, submit calls store
- `TokenRevealDialog.test.tsx` — token displays, copy button copies (mock
  clipboard)

## 5. Branding (spec §9.3 — v0 minimal)

Three edits, ~25 LOC total:

### 5.1 `web/src/components/BrandLogo.tsx`

Rewrite to render plain "cocli" wordmark in Inter, weight 500. Use Tailwind:
`text-foreground font-medium font-sans tracking-tight`. Props `size:
'sm'|'md'|'lg'` map to text classes. No "local" subscript (clean).

### 5.2 `web/public/favicon.svg` (new)

Single-color "c" SVG, viewBox 32×32, `fill="currentColor"`, system-aware
via media query inside SVG (`prefers-color-scheme`).

### 5.3 `web/index.html`

- `<title>cocli local</title>`
- `<meta name="description" content="Multi-agent Claude on your machine.">`
- `<link rel="icon" type="image/svg+xml" href="/favicon.svg">`
- Drop any legacy `<link rel="icon" href="/favicon.ico">` (delete the old
  PNG/ICO from `public/` if present)

No Tailwind config edit. No new color tokens. Theme tokens untouched.

## 6. ESLint cleanup

After §2 strip, residual errors (verified locally before strip ran: 33 total,
~17 in files that will be deleted, ~16 in files that survive):

| File | Errors | Fix |
|---|---|---|
| `components/chat/ChannelMemoryPanel.test.tsx` | 2 `no-explicit-any` | `as unknown as Foo` |
| `components/sidebar/CreateChannelDialog.test.tsx` | 4 `no-explicit-any` | as above |
| `stores/memoryStore.test.ts` | 9 `no-explicit-any` | as above; define minimal mock types |
| `theme/__tests__/useTheme.test.tsx` | 1 `no-empty` | replace `() => {}` with `() => { /* intentional */ }` |

No change to `eslint.config.js`. Goal: `npm run lint` exits 0. Verify by
running both `npm run lint` (in `web/`, which lints `web/` + `shared/`) and
fresh `npm test`.

## 7. shared/api/client.ts contract (spec §4.1)

The client is the de-facto spec §4.1 contract that the Rust `cocli-api`
crate will implement during M0.0.1+. After this slice, the client's exported
shape should look like:

```ts
// Auth
setApiKey(key: string)                  // stays — used when token mode on
getApiKey(): string

// Connection helpers
setUnauthorizedHandler(fn)
getInflight()
subscribeInflight(fn)

// Resources (URLs MUST match spec §4.1)
version.get()                          // GET  /api/version
health.get()                           // GET  /api/health
settings.get() / patch(payload)        // GET/PATCH /api/settings — replaces `prefs.*`

channels.list/create/get/update/delete/getMembers/addMember/removeMember/archive
                                       // GET/POST /api/channels (no zoneId)
                                       // GET/PATCH/DELETE /api/channels/:id
                                       // GET/POST/DELETE /api/channels/:id/members
messages.list(channelId, ...)          // GET/POST /api/channels/:id/messages
messages.send(channelId, content)
messages.markRead(channelId, seq)
messages.blockAction(messageId, payload)

agents.list/create/get/update/start/stop/delete/cancelTurn/steerTurn/forkThread
                                       // GET/POST /api/agents
agents.runtimes()                      // returns ['claude'] in v0
agentActivity.list(agentId, ...)       // GET /api/agents/:id/activity
agentWorkspace.listDir/readFile/reset  // workspace endpoints (spec §4.1)
agentSessions.list/current(agentId)    // sessions endpoints
agentTurns.list/get(agentId, turnId)   // turns

tasks.list/create/claim/unclaim/updateStatus/getDependencies/executionTimeline
                                       // task endpoints
history.list(params)                   // GET /api/history (de-zoned)
search.messages(q, ...)                // GET /api/messages/search
dm.list/createOrGet                    // GET/POST /api/dm
threads.getOrCreate/list/listAll/setDone
pins.list/pin/unpin
reactions.list/add/remove
bookmarks.list/create/remove
presence.list/setViewingChannel
attachments.upload(file)
exportData.messagesUrl/tasksUrl
memory.getAgentIndex/getAgentTopic/getChannelIndex/getChannelTopic
overflowStats.list()

// NEW (spec §4.1 + §4.4)
plugins.list()                         // GET    /api/plugins
plugins.register(name, capabilities)   // POST   /api/plugins → { plugin, token }
plugins.revoke(id)                     // DELETE /api/plugins/:id
```

**Deleted exports**: `zones`, `zoneMembers`, `daemons`, `chatrsCredentials`,
`chatrsAgentBinding`, `users`, `auth`, `invites`, `zoneInvites`,
`zoneSkillLibrary`, `agentSkills`, `runtimes` (the SaaS zoneId-scoped
runtimes endpoint; replaced by `agents.runtimes()` if needed), `pushTokens`,
`zoneTasks`.

**Auth header rename**: `X-API-Key` → `X-Cocli-Token` (spec §6.1 line 829).
The header is only sent when token mode is on; default is no header (local
unauthenticated). `setApiKey` keeps its name for backwards-compat with the
sibling fields in BrandStorage; internally it sets the `X-Cocli-Token`
value.

**Stores-only mode**: every method body short-circuits when
`import.meta.env.VITE_USE_MOCK === 'true'`:

```ts
async function request<T>(path, options): Promise<T> {
  if (import.meta.env.VITE_USE_MOCK === 'true') {
    return mockHandler<T>(path, options)
  }
  // ... existing fetch logic
}
```

`mockHandler` lives in `shared/api/mock.ts` (new, ~80 LOC stub). It returns
empty arrays / nulls / 204s for everything; the only routes that get real
mock data are:
- `GET /api/channels` → `[{ id: 'general', name: 'general', type: 'channel',
  ... }]` (single hardcoded channel so the post-wizard navigate works)
- `GET /api/version` → `{ version: '0.0.0-mock', commit: 'mock' }`
- `GET /api/health` → `204`

Everything else is `Promise.resolve([])` or `Promise.resolve(undefined)`.
The wizard and plugin manager bypass the client entirely (they go straight
to their zustand stores), so the mock layer doesn't need to grow.

Set `VITE_USE_MOCK=true` in a new `web/.env.local.example` and document in
`web/README.md`.

## 8. shared/types/index.ts trim

### 8.1 Delete

`Zone`, `ZoneMember`, `ZoneInvite`, `TenantProviderKey`,
`AgentProviderBinding`, `CreateCredentialInput`, `UpsertBindingInput`,
`SkillView`, `SkillFileEntry`, `SkillLibraryEntry`, `SkillLibraryFileMeta`,
`SkillLibraryImportResponse`, `SkillLibraryReinstallResponse`,
`MachineVersionStatus`, `Invite` (user-invite, not to be confused with
plugin), `LegacyTaskStatus | ZoneTaskStatus` collapse to single
`TaskStatus = 'pending' | 'claimed' | 'in_progress' | 'completed' |
'failed'`.

### 8.2 Modify

- `Agent` — drop `zoneId`, `machineId` (single-machine local).
- `Machine` — drop entirely (no daemon manager in v0; spec §1.2 says local
  is single-binary). All `Machine` references get removed during edit.
  Known consumers to clean up or delete in the same pass:
  `web/src/stores/machineStatusStore.ts` (+ its tests), any sidebar/agent
  component that displays "host machine" hints. If a small subset still
  needs to declare a "this machine" hostname for display, replace with a
  trivial local constant — do not preserve the `Machine` type.
- `User` — drop `role`, `hasPassword`, `email`. The shim only needs
  `{id, name, displayName}`.

### 8.3 Add

```ts
export type PluginCapability = 'inbound-bridge' | 'outbound-bridge'

export interface Plugin {
  id: string
  name: string
  capabilities: PluginCapability[]
  createdAt: string
  lastSeenAt: string | null
}

export interface PluginRegistration {
  plugin: Plugin
  token: string  // plaintext, server returns ONCE per spec §4.4
}
```

## 9. Test impact + risks

### 9.1 Tests that will be deleted (because their target files are)

- All `components/zone/*.test.tsx`
- `components/sidebar/{ZoneMembersPanel,ProviderKeysTab,CreateZoneDialog,
  CreateKeyDialog,UserList,AddDaemonDialog}.test.tsx` (where exists)
- `stores/{zoneStore,zoneAdminStore,zoneTaskBoardStore,
  chatrsCredentialsStore}.test.ts`

### 9.2 Tests that survive

- `stores/{channelStore,messageStore,agentStore,taskStore,...}.test.ts` —
  must be updated to call methods without `zoneId` argument.
- `components/sidebar/CreateChannelDialog.test.tsx` — keep, fix `any`.
- `components/chat/ChannelMemoryPanel.test.tsx` — keep, fix `any`.
- `theme/__tests__/useTheme.test.tsx` — keep, fix `no-empty`.

### 9.3 Tests to add

- `wizardStore.test.ts` + per-step tests
- `pluginsStore.test.ts` + per-dialog tests + `PluginsPage.test.tsx`
- `userStore.test.ts` — verify shim returns the hardcoded user, does not
  fetch

### 9.4 Risk: silent route breakage

Refactoring router + flat URL structure can leave bookmarks/links broken.
Mitigation: router has a final `path: '*'` catchall that redirects to `/`
(channel index). No deep-link compatibility is owed — repo is private until
M0.0.4.

### 9.5 Risk: zoneId leak

Some component or store may have a hidden `zoneId` constant that we miss.
Detection: `git grep -nE 'zone[A-Z]|/zones/|zoneId|zoneSlug'` after strip
should produce zero hits in `web/` and `shared/`. CI guard: add a small
shell check to the lint script that fails on any such grep match.

### 9.6 Risk: shared/api/client.ts mock drift

The mock stub returns empty data, so any component that does NOT have an
empty-state code path will crash. Mitigation: smoke test by running
`npm run build && vite preview` and clicking every reachable route. Any
crash gets either an empty-state added or the route excluded for now.

### 9.7 Risk: ESLint suppressions

Some `eslint-disable` comments may be on lines that get deleted; the
"unused eslint-disable" warning catches these. After strip, run lint, fix
any new warnings.

## 10. Acceptance criteria

A reviewer should be able to verify the slice by running:

```bash
cd web/
npm install                            # if needed
npm run lint                           # MUST exit 0
npm test                               # MUST pass; new wizard/plugin tests included
VITE_USE_MOCK=true npm run dev
# - app loads at localhost:5173
# - First-run wizard overlays
# - Click Detect → green check
# - Click Next → CreateAgentStep
# - Enter "@assistant", model=sonnet → Next
# - Click "Open #general →" → land in /channel/general (empty channel)
# - Sidebar shows "@assistant" agent
# - Navigate to /settings/plugins
# - Empty state visible
# - Click "Register plugin", name="telegram-bot", check inbound-bridge → Register
# - Token reveal modal shows a UUID + Copy button
# - Click "I've saved it" → row appears
# - Click trash icon → confirm → row vanishes
# - Refresh page → plugin reappears from localStorage
# - localStorage.clear() + refresh → wizard reappears
npm run build                          # MUST succeed
```

Plus:
- `git grep -nE 'zone[A-Z]|/zones/|zoneId|zoneSlug|chatrsCredentials|
  LoginPage|InviteSignup|ProviderKey|zoneAdmin|zoneTaskBoard|SkillsLibrary|
  ZoneMembers|ZoneSwitcher|AddDaemon|CreateZone|CreateKey|UserList' web/src
  shared/` returns zero matches.
- `git grep -nE 'X-API-Key' web/src shared/` returns zero matches (renamed
  to `X-Cocli-Token`).
- `git diff --stat` shows net `-2000 ± 500` LOC (delete-heavy).

## 11. Open questions / deferred

### 11.1 Deferred to M0.0.1

- Real `cocli-api` crate implements the contract in §7. The mock stub in
  `shared/api/mock.ts` deletes (or `VITE_USE_MOCK=false` becomes the
  default) once the binary serves on `:8080`.
- Wizard step 1 "Detect" calls real `/api/system/detect-claude` (new
  endpoint not in spec §4.1; add it then).
- Wizard step 2 commits via real `POST /api/agents` instead of in-memory.

### 11.2 Deferred to M0.0.4 (soft launch polish)

- Settings hub at `/settings` with subnav (Plugins becomes one of N pages).
- Plugin manager `last-seen` populated from real `/ws` events.
- i18n coverage audit (zh strings may have stale SaaS references after
  strip; touch up).
- Demo screenshots per spec §9.3 (3 required: chat + activity feed +
  plugin settings).

### 11.3 Out of scope (record but do not act)

- Custom branding palette (spec §9.3 says v0 minimal).
- Mobile/responsive polish (cocli local is desktop-first per spec §0.4).
- Real CLI flag `--skip-wizard --claude-path=... --auto-create-agent`
  (needs Rust binary cooperation; URL `?skip-wizard=1` covers screenshot
  needs).
- MSW or any network-mock infrastructure (stores-only chosen).

## 12. Execution sketch (handoff to writing-plans)

Bite-sized task buckets for the plan, in order:

1. **Delete pass** — git rm all §2.1 files, commit. CI should still build
   (broken imports will be caught next step).
2. **Router flatten + userStore shim** — fix import cascade; app should
   compile.
3. **shared/types trim + shared/api/client.ts rewrite + mock stub** — biggest
   single chunk; commit when `tsc -b` is green.
4. **Store consumers de-zoned** — fix each store to drop `zoneId` arg.
5. **First-run wizard scaffold** — store + steps + mount.
6. **Plugin manager scaffold** — store + page + dialogs + route.
7. **Branding edits** — BrandLogo + favicon + index.html.
8. **ESLint zero pass** — fix the 16 residual errors.
9. **Tests** — wizard + plugins + userStore + de-zoned store updates.
10. **Final acceptance** — run §10 checklist; commit a CHANGELOG note;
    push branch; open PR against `main`.

Each step is a separate commit per `git-and-the-art-of-clear-commits`
discipline.

# web/ multi-tenant strip + first-run wizard + plugin manager mockup + branding + ESLint cleanup — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Strip multi-tenant artefacts from `web/` + `shared/`, add a stores-only first-run wizard and a full-CRUD plugin manager UI mockup, swap to cocli-local branding, drive ESLint to zero. Outcome: `npm run dev` (`VITE_USE_MOCK=true`) walks a new user through wizard → `/channel/general` and `/settings/plugins`, fully offline.

**Architecture:** Delete + flatten + shim (not feature-flag hide). The 30-LOC `userStore` shim returns a hardcoded single user; the router collapses `/z/:zoneSlug/*` to root and removes `/login` + `/invite/:code`; `shared/api/client.ts` is rewritten as the spec §4.1 contract that M0.0.1 backend will implement, with a `VITE_USE_MOCK=true` short-circuit to a tiny `shared/api/mock.ts` stub.

**Tech Stack:** Vite 8 + React 19 + TypeScript ~5.9 + Tailwind 4 + zustand 5 + i18next + vitest + ESLint 9 / typescript-eslint 8 / eslint-plugin-react-hooks 7.

**Spec:** `docs/superpowers/specs/2026-05-21-web-multitenant-strip-and-polish-design.md`
**Parent spec:** `~/code/1HzAi/docs/superpowers/specs/2026-05-21-cocli-oss-launch-design.md` (§6.1, §4.1, §9.3)
**Worktree:** `.claude/worktrees/web-multitenant-strip-and-polish` (branch `worktree-web-multitenant-strip-and-polish`)
**Cwd:** all commands assume `pwd` is the worktree root unless stated.

**Pre-flight (do once before Task 1):**

```bash
cd .claude/worktrees/web-multitenant-strip-and-polish
[ -L web/node_modules ] || ln -s ../../../web/node_modules web/node_modules
cd web && ./node_modules/.bin/tsc -b --dry 2>&1 | head -5  # confirms toolchain reachable
```

Expected: no errors from tsc dry run (workspace is in clean state).

---

## Phase 1 — Strip pass (Tasks 1-6)

These are bulk deletes. No new tests; verification is "tsc + lint don't get worse than the planned interim broken state". Each task commits independently so a reviewer can audit one bucket at a time.

### Task 1: Delete zone-* and chatrs stores

**Files:**
- Delete: `web/src/stores/zoneStore.ts`
- Delete: `web/src/stores/zoneAdminStore.ts`
- Delete: `web/src/stores/zoneTaskBoardStore.ts`
- Delete: `web/src/stores/chatrsCredentialsStore.ts`
- Delete: `web/src/stores/agentSkillStore.ts`
- Delete: `web/src/stores/agentSkillStore.test.ts`
- Delete: `web/src/stores/machineStatusStore.ts`
- Delete: `web/src/stores/devToolsStore.ts`

- [ ] **Step 1: Remove the files**

```bash
git rm web/src/stores/zoneStore.ts \
       web/src/stores/zoneAdminStore.ts \
       web/src/stores/zoneTaskBoardStore.ts \
       web/src/stores/chatrsCredentialsStore.ts \
       web/src/stores/agentSkillStore.ts \
       web/src/stores/agentSkillStore.test.ts \
       web/src/stores/machineStatusStore.ts \
       web/src/stores/devToolsStore.ts
```

- [ ] **Step 2: Verify removal**

```bash
ls web/src/stores/ | grep -E '^(zone|chatrs|agentSkill|machineStatus|devTools)'
```

Expected: no output.

- [ ] **Step 3: Commit (broken-state OK — fixed by end of Phase 3)**

```bash
git commit -m "strip(stores): remove zone/chatrs/agentSkill/machineStatus/devTools stores"
```

---

### Task 2: Delete zone-related components

**Files:**
- Delete entire dir: `web/src/components/zone/` (11 files)
- Delete: `web/src/components/sidebar/ZoneSwitcher.tsx`
- Delete: `web/src/components/sidebar/ZoneMembersPanel.tsx`
- Delete: `web/src/components/sidebar/ZoneThemeSelect.tsx`
- Delete: `web/src/components/sidebar/CreateZoneDialog.tsx`
- Delete: `web/src/components/sidebar/CreateKeyDialog.tsx`
- Delete: `web/src/components/sidebar/ProviderKeysTab.tsx`
- Delete: `web/src/components/sidebar/AddDaemonDialog.tsx`
- Delete: `web/src/components/sidebar/UserList.tsx`
- Delete: `web/src/components/sidebar/InviteLinks.tsx`

- [ ] **Step 1: Remove zone component directory + sidebar files**

```bash
git rm -r web/src/components/zone
git rm web/src/components/sidebar/ZoneSwitcher.tsx \
       web/src/components/sidebar/ZoneMembersPanel.tsx \
       web/src/components/sidebar/ZoneThemeSelect.tsx \
       web/src/components/sidebar/CreateZoneDialog.tsx \
       web/src/components/sidebar/CreateKeyDialog.tsx \
       web/src/components/sidebar/ProviderKeysTab.tsx \
       web/src/components/sidebar/AddDaemonDialog.tsx \
       web/src/components/sidebar/UserList.tsx \
       web/src/components/sidebar/InviteLinks.tsx
```

- [ ] **Step 2: Verify removal**

```bash
ls web/src/components/zone 2>&1
ls web/src/components/sidebar/ | grep -E '^(Zone|CreateZone|CreateKey|ProviderKeys|AddDaemon|UserList|InviteLinks)'
```

Expected: first command "No such file or directory"; second command no output.

- [ ] **Step 3: Commit**

```bash
git commit -m "strip(components): remove zone/* + sidebar/{Zone*,CreateZone,CreateKey,ProviderKeys,AddDaemon,UserList,InviteLinks}"
```

---

### Task 3: Delete auth + skills components

**Files:**
- Delete: `web/src/components/LoginPage.tsx`
- Delete: `web/src/components/InviteSignup.tsx`
- Delete: `web/src/components/UserProfile.tsx`
- Delete: `web/src/components/agents/SkillsTab.tsx`
- Delete: `web/src/components/agents/SkillsTab.test.tsx`
- Delete: `web/src/components/agents/SkillViewModal.tsx`
- Delete: `web/src/components/agents/SkillViewModal.test.tsx`
- Delete: `web/src/components/agents/VersionStatusBadge.tsx`

- [ ] **Step 1: Remove the files**

```bash
git rm web/src/components/LoginPage.tsx \
       web/src/components/InviteSignup.tsx \
       web/src/components/UserProfile.tsx \
       web/src/components/agents/SkillsTab.tsx \
       web/src/components/agents/SkillsTab.test.tsx \
       web/src/components/agents/SkillViewModal.tsx \
       web/src/components/agents/SkillViewModal.test.tsx \
       web/src/components/agents/VersionStatusBadge.tsx
```

- [ ] **Step 2: Verify removal**

```bash
ls web/src/components/{LoginPage,InviteSignup,UserProfile}.tsx 2>&1 | wc -l
ls web/src/components/agents/{SkillsTab,SkillViewModal,VersionStatusBadge}* 2>&1 | wc -l
```

Expected: both `8` (8 "No such file" error lines = all gone — actually 5; we just want non-zero error output).

Better assertion:

```bash
for f in web/src/components/LoginPage.tsx web/src/components/InviteSignup.tsx \
         web/src/components/UserProfile.tsx \
         web/src/components/agents/SkillsTab.tsx web/src/components/agents/SkillViewModal.tsx \
         web/src/components/agents/VersionStatusBadge.tsx; do
  [ ! -e "$f" ] || { echo "STILL THERE: $f"; exit 1; }
done && echo "all gone"
```

Expected: `all gone`.

- [ ] **Step 3: Commit**

```bash
git commit -m "strip(components): remove LoginPage/InviteSignup/UserProfile + agents/{SkillsTab,SkillViewModal,VersionStatusBadge}"
```

---

### Task 4: Delete daemons + wiki + devtools dirs

**Files:**
- Delete dir: `web/src/components/daemons/`
- Delete dir: `web/src/components/wiki/`
- Delete dir: `web/src/components/devtools/`

- [ ] **Step 1: Remove the directories**

```bash
git rm -r web/src/components/daemons \
          web/src/components/wiki \
          web/src/components/devtools
```

- [ ] **Step 2: Verify removal**

```bash
ls web/src/components/{daemons,wiki,devtools} 2>&1 | grep -c "No such"
```

Expected: `3`.

- [ ] **Step 3: Commit**

```bash
git commit -m "strip(components): remove daemons/ + wiki/ + devtools/ dirs"
```

---

### Task 5: Delete multi-tenant routes

**Files:**
- Delete: `web/src/routes/LoginRoute.tsx`
- Delete: `web/src/routes/InviteRoute.tsx`
- Delete: `web/src/routes/ZoneDevToolsRoute.tsx`
- Delete: `web/src/routes/ZonePanelRoute.tsx`
- Delete: `web/src/routes/DaemonDetailRoute.tsx`
- Delete: `web/src/routes/LegacyDevtoolsRedirect.tsx`

- [ ] **Step 1: Remove the files**

```bash
git rm web/src/routes/LoginRoute.tsx \
       web/src/routes/InviteRoute.tsx \
       web/src/routes/ZoneDevToolsRoute.tsx \
       web/src/routes/ZonePanelRoute.tsx \
       web/src/routes/DaemonDetailRoute.tsx \
       web/src/routes/LegacyDevtoolsRedirect.tsx
```

- [ ] **Step 2: Verify remaining routes are exactly the two we keep**

```bash
ls web/src/routes/
```

Expected:

```
AgentRoute.tsx
ChannelRoute.tsx
```

- [ ] **Step 3: Commit**

```bash
git commit -m "strip(routes): remove Login/Invite/ZoneDevTools/ZonePanel/DaemonDetail/LegacyDevtoolsRedirect"
```

---

### Task 6: Strip-pass interim state — record broken-imports baseline

After Phase 1, many files import deleted symbols (`useZoneStore`, `LoginPage`, `ProviderKeysTab`, etc.). `tsc -b` will error heavily. This task captures the baseline for later comparison.

**Files:**
- Modify (write report only, not source): create `/tmp/cocli-strip-baseline.txt` (NOT committed)

- [ ] **Step 1: Capture broken-import baseline**

```bash
cd web
./node_modules/.bin/tsc -b 2>&1 | tail -100 > /tmp/cocli-strip-baseline.txt
echo "errors: $(grep -c 'error TS' /tmp/cocli-strip-baseline.txt)"
```

Expected: a non-zero error count (probably 80-200). This is the interim broken state — DO NOT try to fix yet; Phase 3 fixes everything in one go.

- [ ] **Step 2: No commit (baseline is local-only)**

Just verify the file exists for reference:

```bash
ls -la /tmp/cocli-strip-baseline.txt
```

---

## Phase 2 — Types + API client + mock (Tasks 7-10)

After Phase 2: `shared/` builds clean. `web/` is still broken — fixed in Phase 3.


This phase reshapes `shared/` to the spec §4.1 contract. After Phase 2, `shared/` builds clean (`tsc -b` in `shared/` exits 0). `web/` is still broken — fixed in Phase 3.

### Task 7: Trim shared/types/index.ts

**Files:**
- Modify: `shared/types/index.ts` (519 LOC → ~330 LOC)

- [ ] **Step 1: Read current file**

```bash
wc -l shared/types/index.ts
```

Expected: `519`.

- [ ] **Step 2: Apply the trim**

Open `shared/types/index.ts` and apply these surgical edits:

**Delete entirely** (these interface/type blocks):
- `Invite` (the user-invite, lines ~319-327)
- `Zone` (~329-336)
- `ZoneMember` (~338-350)
- `ZoneInvite` (~352-365)
- `TenantProviderKey` (~369-380)
- `CreateCredentialInput` (~382-388)
- `AgentProviderBinding` (~390-400)
- `UpsertBindingInput` (~402-408)
- `MachineVersionStatus` (~410)
- `Machine` (~412-437)
- `SkillLibraryEntry` (~460-477)
- `SkillLibraryFileMeta` (~479-483)
- `SkillLibraryImportResponse` (~485-489)
- `SkillLibraryReinstallResponse` (~491-494)
- `SkillView` (~498-511)
- `SkillFileEntry` (~514-518)

**Modify**:
- `Agent` (~69-101): delete the lines `machineId?: string` and `zoneId: string`
- `TaskStatus` (~196-198): collapse `LegacyTaskStatus | ZoneTaskStatus` to a single union. Replace lines 196-198 with:

```ts
export type TaskStatus = 'pending' | 'claimed' | 'in_progress' | 'completed' | 'failed'
```

- `User` (~1-9): trim to:

```ts
export interface User {
  id: string
  name: string
  displayName?: string
}
```

**Append at end of file**:

```ts
// Plugins (cocli OSS spec §4.1 + §4.4)
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
  token: string  // plaintext; server returns ONCE per spec §4.4
}
```

- [ ] **Step 3: Verify shared/types/index.ts compiles**

```bash
cd shared && ../web/node_modules/.bin/tsc --noEmit
```

Expected: exit 0, no output.

- [ ] **Step 4: Commit**

```bash
cd ..  # back to worktree root
git add shared/types/index.ts
git commit -m "shared(types): trim Zone/Skill/Machine/Invite/TenantProvider*; add Plugin*"
```

---

### Task 8: Trim shared/api/client.ts (delete SaaS exports)

**Files:**
- Modify: `shared/api/client.ts` (777 LOC → ~600 LOC after this task; Task 9 finishes flattening)

This task ONLY deletes whole `export const X = { ... }` blocks. Task 9 reshapes the survivors.

- [ ] **Step 1: Delete these top-level exports + their import lines**

In `shared/api/client.ts`, delete these blocks entirely:

- `export const zones = { ... }` (lines ~161-181)
- `export const zoneMembers = { ... }` (~183-207)
- `export const daemons = { ... }` (~209-220)
- `export const chatrsCredentials = { ... }` (~222-236)
- `export const chatrsAgentBinding = { ... }` (~238-254)
- `export const users = { ... }` (~256-268)
- `export const zoneTasks = { ... }` (~529-549)
- `export const agentSkills = { ... }` (~563-589)
- `export const runtimes = { ... }` (~591-597)
- `export const auth = { ... }` (~663-682)
- `export const invites = { ... }` (~684-693)
- `export const zoneInvites = { ... }` (~695-713)
- `export const zoneSkillLibrary = { ... }` (~715-753)
- `export const pushTokens = { ... }` (~755-776)

In the top `import type { ... }` block (lines ~1-35), remove these names (leave the rest):

- `AgentProviderBinding`
- `CreateCredentialInput`
- `Invite`
- `Machine`
- `SkillFileEntry`
- `SkillLibraryEntry`
- `SkillLibraryFileMeta`
- `SkillLibraryImportResponse`
- `SkillLibraryReinstallResponse`
- `SkillView`
- `TenantProviderKey`
- `UpsertBindingInput`
- `User` (re-added in Task 9 only if needed)
- `Zone`
- `ZoneInvite`
- `ZoneMember`

- [ ] **Step 2: Verify count of remaining exports**

```bash
grep -cE '^export (const|class|function) ' shared/api/client.ts
```

Expected: around 20-25 (down from ~30).

- [ ] **Step 3: Verify shared/api/client.ts still compiles standalone**

Note: it will NOT compile yet because it still references zoneId in surviving methods. Skip tsc here; commit interim state.

- [ ] **Step 4: Commit**

```bash
git add shared/api/client.ts
git commit -m "shared(api): delete zones/users/auth/skills/daemons/credentials/invites/pushTokens exports"
```

---

### Task 9: Flatten shared/api/client.ts URLs (drop zoneId from survivors)

**Files:**
- Modify: `shared/api/client.ts` (~600 LOC → ~470 LOC after this task)

This task de-zones every surviving method (`channels`, `dm`, `agents`, `history`, `search`, `threads`, `prefs`, `tasks`) and renames `X-API-Key` → `X-Cocli-Token`. Adds `plugins` + `version` + `health` + `settings`.

- [ ] **Step 1: Rename auth header**

In `shared/api/client.ts`, find:

```ts
'X-API-Key': sentKey,
```

There are two occurrences (one in `request()`, one in `attachments.upload`). Replace both with:

```ts
'X-Cocli-Token': sentKey,
```

Also rename the localStorage key — find:

```ts
localStorage.setItem(storageKey('api-key'), key)
// ...
apiKey = localStorage.getItem(storageKey('api-key')) || ''
```

Change both to `storageKey('token')`.

- [ ] **Step 2: De-zone channel APIs**

Replace `export const channels = { ... }` (the surviving block) with:

```ts
// Channels
export const channels = {
  list: (opts?: { includeArchived?: boolean }) => {
    const q = opts?.includeArchived ? '?includeArchived=true' : ''
    return request<Channel[]>(`/api/channels${q}`)
  },
  create: (name: string, description?: string) =>
    request<Channel>(`/api/channels`, {
      method: 'POST',
      body: JSON.stringify({ name, description }),
    }),
  get: (id: string) => request<Channel>(`/api/channels/${id}`),
  update: (id: string, data: { displayName?: string; description?: string }) =>
    request<Channel>(`/api/channels/${id}`, { method: 'PATCH', body: JSON.stringify(data) }),
  delete: (id: string) => request<void>(`/api/channels/${id}`, { method: 'DELETE' }),
  getMembers: (id: string) =>
    request<{ id: string; memberId: string; memberType: string }[]>(
      `/api/channels/${id}/members`
    ),
  addMember: (id: string, memberId: string, memberType: string) =>
    request<void>(`/api/channels/${id}/members`, {
      method: 'POST',
      body: JSON.stringify({ memberId, memberType }),
    }),
  removeMember: (id: string, memberId: string, memberType: string) =>
    request<void>(`/api/channels/${id}/members`, {
      method: 'DELETE',
      body: JSON.stringify({ memberId, memberType }),
    }),
  listResponderPolicies: (id: string) =>
    request<ChannelResponderPolicy[]>(`/api/channels/${id}/responder-policies`),
  upsertResponderPolicy: (id: string, agentId: string, role: ResponderRole, priorityWeight = 0) =>
    request<ChannelResponderPolicy>(`/api/channels/${id}/responder-policies/${agentId}`, {
      method: 'PUT',
      body: JSON.stringify({ role, priorityWeight }),
    }),
  getResponderMode: (id: string) =>
    request<ChannelResponderModeState>(`/api/channels/${id}/responder-mode`),
  updateResponderMode: (id: string, mode: ResponderMode) =>
    request<ChannelResponderModeState>(`/api/channels/${id}/responder-mode`, {
      method: 'PUT',
      body: JSON.stringify({ mode }),
    }),
  archive: (id: string, archived: boolean) =>
    request<{ ok: true; archived: boolean }>(`/api/channels/${id}/archive`, {
      method: 'PATCH',
      body: JSON.stringify({ archived }),
    }),
}
```

- [ ] **Step 3: De-zone dm, history, search, threads, agents, tasks**

Replace these blocks (drop `zoneId` first arg from each):

```ts
// DMs
export const dm = {
  list: () => request<Channel[]>(`/api/dm`),
  createOrGet: (peerName: string, peerType?: string) =>
    request<Channel>(`/api/dm`, {
      method: 'POST',
      body: JSON.stringify({ peerName, peerType }),
    }),
}

// History
export const history = {
  list: (params: HistoryQuery = {}) => {
    const qs = new URLSearchParams()
    if (params.channelId) qs.set('channelId', params.channelId)
    if (params.q) qs.set('q', params.q)
    if (params.from) qs.set('from', params.from)
    if (params.to) qs.set('to', params.to)
    if (params.senderType) qs.set('senderType', params.senderType)
    if (params.senderId) qs.set('senderId', params.senderId)
    qs.set('page', String(params.page ?? 1))
    qs.set('pageSize', String(params.pageSize ?? 30))
    return request<HistoryResult>(`/api/history?${qs.toString()}`)
  },
}

// Search
export const search = {
  messages: (q: string, limit?: number, options?: { signal?: AbortSignal }) => {
    const qs = new URLSearchParams({ q })
    if (limit) qs.set('limit', String(limit))
    return request<{ messages: Message[] }>(`/api/messages/search?${qs}`, {
      signal: options?.signal,
    })
  },
}

// Threads
export const threads = {
  getOrCreate: (channelId: string, messageId: string) =>
    request<Channel>(`/api/channels/${channelId}/messages/${messageId}/thread`, { method: 'POST' }),
  list: (channelId: string) =>
    request<Channel[]>(`/api/channels/${channelId}/threads`),
  listAll: () => request<{ threads: ThreadSummary[] }>(`/api/threads`),
  setDone: (threadId: string, done: boolean) =>
    request<{ id: string; done: boolean }>(`/api/threads/${threadId}/done`, {
      method: 'PATCH',
      body: JSON.stringify({ done }),
    }),
}

// Agents
export const agents = {
  list: () => request<Agent[]>(`/api/agents`),
  create: (data: {
    name: string
    runtime?: string
    model?: string
    description?: string
    workingRuntime?: string
    workingModel?: string
    chatOnly?: boolean
  }) => request<Agent>(`/api/agents`, { method: 'POST', body: JSON.stringify(data) }),
  get: (id: string) => request<Agent>(`/api/agents/${id}`),
  update: (id: string, data: Partial<Agent>) =>
    request<Agent>(`/api/agents/${id}`, { method: 'PATCH', body: JSON.stringify(data) }),
  start: (id: string) => request<void>(`/api/agents/${id}/start`, { method: 'POST' }),
  stop: (id: string, force?: boolean) =>
    request<void>(`/api/agents/${id}/stop${force ? '?force=true' : ''}`, { method: 'POST' }),
  cancelTurn: (id: string) =>
    request<void>(`/api/agents/${id}/turn/cancel`, { method: 'POST' }),
  steerTurn: (id: string, input: string) =>
    request<void>(`/api/agents/${id}/turn/steer`, {
      method: 'POST',
      body: JSON.stringify({ input }),
    }),
  forkThread: (id: string) =>
    request<void>(`/api/agents/${id}/thread/fork`, { method: 'POST' }),
  delete: (id: string) => request<void>(`/api/agents/${id}`, { method: 'DELETE' }),
  runtimes: () => request<string[]>(`/api/agents/runtimes`),
}

// Tasks (channel-scoped; spec §4.1 only)
export const tasks = {
  list: (channelId: string, status?: string) => {
    const qs = status ? `?status=${status}` : ''
    return request<Task[]>(`/api/channels/${channelId}/tasks${qs}`)
  },
  create: (channelId: string, title: string) =>
    request<Task>(`/api/channels/${channelId}/tasks`, {
      method: 'POST',
      body: JSON.stringify({ title }),
    }),
  claim: (channelId: string, taskNumber: number) =>
    request<Task>(`/api/channels/${channelId}/tasks/${taskNumber}/claim`, { method: 'POST' }),
  unclaim: (channelId: string, taskNumber: number) =>
    request<Task>(`/api/channels/${channelId}/tasks/${taskNumber}/unclaim`, { method: 'POST' }),
  updateStatus: (channelId: string, taskNumber: number, status: Task['status']) =>
    request<Task>(`/api/channels/${channelId}/tasks/${taskNumber}/status`, {
      method: 'POST',
      body: JSON.stringify({ status }),
    }),
  getDependencies: (channelId: string, taskNumber: number) =>
    request<{ taskNumber: number; dependsOn: number[] }>(
      `/api/channels/${channelId}/tasks/${taskNumber}/dependencies`
    ),
  executionTimeline: (channelId: string, taskNumber: number) =>
    request<TaskExecutionTimeline>(`/api/channels/${channelId}/tasks/${taskNumber}/execution`),
}
```

- [ ] **Step 4: Rename prefs → settings; add version, health, plugins**

Find `export const prefs = { ... }` block and replace with:

```ts
// Settings (spec §4.1 — replaces SaaS user-prefs)
export const settings = {
  get: () => request<Record<string, unknown>>(`/api/settings`),
  patch: (payload: Record<string, unknown>) =>
    request<{ ok: true }>(`/api/settings`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    }),
}

// Version + health (spec §4.1)
export const version = {
  get: () => request<{ version: string; commit: string; buildTime?: string }>(`/api/version`),
}

export const health = {
  get: () => request<void>(`/api/health`),
}

// Plugins (spec §4.1 + §4.4)
export const plugins = {
  list: () => request<Plugin[]>(`/api/plugins`),
  register: (name: string, capabilities: PluginCapability[]) =>
    request<PluginRegistration>(`/api/plugins`, {
      method: 'POST',
      body: JSON.stringify({ name, capabilities }),
    }),
  revoke: (id: string) => request<void>(`/api/plugins/${id}`, { method: 'DELETE' }),
}
```

Also update the top `import type { ... }` block to add `Plugin, PluginCapability, PluginRegistration` and remove `User` (no longer referenced).

- [ ] **Step 5: Verify shared/api/client.ts compiles**

```bash
cd shared && ../web/node_modules/.bin/tsc --noEmit
```

Expected: exit 0.

- [ ] **Step 6: Commit**

```bash
cd ..
git add shared/api/client.ts
git commit -m "shared(api): flatten URLs (drop zoneId), add plugins/version/health/settings, rename X-API-Key→X-Cocli-Token"
```

---

### Task 10: Create shared/api/mock.ts + VITE_USE_MOCK short-circuit

**Files:**
- Create: `shared/api/mock.ts`
- Modify: `shared/api/client.ts` (add short-circuit in `request()`)
- Create: `web/.env.local.example`
- Modify: `web/README.md` (document `VITE_USE_MOCK`)

- [ ] **Step 1: Write the failing test**

Create `shared/api/mock.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import { mockHandler } from './mock'

describe('mockHandler', () => {
  it('returns a single hardcoded general channel for GET /api/channels', async () => {
    const result = await mockHandler<{ id: string; name: string }[]>('/api/channels', {})
    expect(result).toHaveLength(1)
    expect(result[0]).toMatchObject({ id: 'general', name: 'general' })
  })

  it('returns version stub for GET /api/version', async () => {
    const result = await mockHandler<{ version: string }>('/api/version', {})
    expect(result.version).toContain('mock')
  })

  it('returns undefined (204) for GET /api/health', async () => {
    const result = await mockHandler<void>('/api/health', {})
    expect(result).toBeUndefined()
  })

  it('returns empty array for unmocked GET paths', async () => {
    const result = await mockHandler<unknown[]>('/api/agents', {})
    expect(result).toEqual([])
  })

  it('returns undefined for unmocked POST/PATCH/DELETE paths', async () => {
    const result = await mockHandler<void>('/api/foo', { method: 'POST' })
    expect(result).toBeUndefined()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd web && ./node_modules/.bin/vitest run ../shared/api/mock.test.ts
```

Expected: FAIL (`Cannot find module './mock'`).

- [ ] **Step 3: Create shared/api/mock.ts**

```ts
// shared/api/mock.ts
//
// Tiny stand-in for the future cocli-api crate. Powers `VITE_USE_MOCK=true`
// dev runs where there's no Rust backend yet. Returns empty/undefined for
// most paths; hardcodes a single `#general` channel + version/health so the
// router can navigate after the first-run wizard.

import type { Channel } from '@shared/types'

const channels: Channel[] = [
  {
    id: 'general',
    name: 'general',
    type: 'channel',
    description: 'Welcome to cocli local',
    createdAt: new Date().toISOString(),
  },
]

export async function mockHandler<T>(path: string, options: RequestInit): Promise<T> {
  const method = (options.method ?? 'GET').toUpperCase()

  if (method === 'GET' && path === '/api/channels') {
    return channels as unknown as T
  }
  if (method === 'GET' && path === '/api/version') {
    return { version: '0.0.0-mock', commit: 'mock' } as unknown as T
  }
  if (method === 'GET' && path === '/api/health') {
    return undefined as T
  }

  if (method === 'GET') {
    return [] as unknown as T
  }
  return undefined as T
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
./node_modules/.bin/vitest run ../shared/api/mock.test.ts
```

Expected: all 5 tests pass.

- [ ] **Step 5: Wire the short-circuit in shared/api/client.ts**

In `shared/api/client.ts`, at the top of the `async function request<T>(...)`, add (after `const requestId = crypto.randomUUID()`):

```ts
if (import.meta.env.VITE_USE_MOCK === 'true') {
  const { mockHandler } = await import('./mock')
  return mockHandler<T>(path, options)
}
```

Also do the same at the top of `attachments.upload`:

```ts
if (import.meta.env.VITE_USE_MOCK === 'true') {
  return { id: 'mock', filename: file.name, url: 'data:,' }
}
```

- [ ] **Step 6: Create web/.env.local.example**

```bash
cat > web/.env.local.example <<'EOF'
# Copy this file to web/.env.local during dev to run the frontend without
# a Rust backend. The shared/api/client.ts request layer short-circuits
# to shared/api/mock.ts.
VITE_USE_MOCK=true
EOF
```

- [ ] **Step 7: Append usage note to web/README.md**

Append at the end of `web/README.md`:

```md

## Backend-less dev (`VITE_USE_MOCK=true`)

Until M0.0.1 ships the Rust backend, run `web/` against the in-process mock:

    cp .env.local.example .env.local   # one-time
    npm run dev                         # vite reads .env.local automatically

The mock returns a single `#general` channel + stub version/health; every other endpoint returns an empty array or `undefined`. The first-run wizard and `/settings/plugins` operate entirely against zustand stores and don't touch the API client.
```

- [ ] **Step 8: Verify build + test still pass**

```bash
cd web && ./node_modules/.bin/vitest run ../shared/api/
```

Expected: 5 tests pass.

- [ ] **Step 9: Commit**

```bash
cd ..
git add shared/api/mock.ts shared/api/mock.test.ts shared/api/client.ts \
        web/.env.local.example web/README.md
git commit -m "shared(api): add mock.ts stub + VITE_USE_MOCK short-circuit"
```

---

## Phase 3 — Surgical edits (Tasks 11-17)

After Phase 3, `tsc -b` in `web/` exits 0 (modulo Phase-4/5 unresolved imports) and existing tests pass.

### Task 11: Rewrite userStore as 30-LOC shim

**Files:**
- Modify: `web/src/stores/userStore.ts` (replace entire file)
- Create: `web/src/stores/userStore.test.ts`

- [ ] **Step 1: Write the failing test first**

Create `web/src/stores/userStore.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import { useUserStore } from './userStore'

describe('useUserStore (single-tenant local shim)', () => {
  it('returns the hardcoded owner user on first read', () => {
    const { user } = useUserStore.getState()
    expect(user).not.toBeNull()
    expect(user?.id).toBe('local')
    expect(user?.name).toBe('owner')
    expect(user?.displayName).toBe('owner')
  })

  it('init() is a no-op', () => {
    const before = useUserStore.getState().user
    useUserStore.getState().init()
    expect(useUserStore.getState().user).toBe(before)
  })

  it('exposes a loading=false synchronously', () => {
    expect(useUserStore.getState().loading).toBe(false)
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd web && ./node_modules/.bin/vitest run src/stores/userStore.test.ts
```

Expected: FAIL (current `userStore.ts` makes network calls / depends on `useZoneStore` which is deleted).

- [ ] **Step 3: Replace userStore.ts with the shim**

Overwrite `web/src/stores/userStore.ts`:

```ts
import { create } from 'zustand'
import type { User } from '@shared/types'

interface UserState {
  user: User
  loading: boolean
  init: () => void
}

const localOwner: User = {
  id: 'local',
  name: 'owner',
  displayName: 'owner',
}

export const useUserStore = create<UserState>(() => ({
  user: localOwner,
  loading: false,
  init: () => {
    /* no-op: single-tenant local has no async user bootstrap */
  },
}))
```

- [ ] **Step 4: Run test to verify it passes**

```bash
./node_modules/.bin/vitest run src/stores/userStore.test.ts
```

Expected: all 3 tests pass.

- [ ] **Step 5: Commit**

```bash
cd ..
git add web/src/stores/userStore.ts web/src/stores/userStore.test.ts
git commit -m "web(userStore): replace with single-tenant local shim"
```

---

### Task 12: De-zone each surviving store

Mechanical edits across multiple store files: every `client.X.Y(zoneId, …)` → `client.X.Y(…)`, every `useZoneStore` import deleted, every `const zoneId = useZoneStore.getState().activeZoneId` deleted along with the `if (!zoneId) return` guard immediately below it.

- [ ] **Step 1: Find every store file still touching zone**

```bash
cd web
grep -lnE "useZoneStore|/api/zones/|zoneTasks|zoneSlug|zoneId" src/stores/*.ts
```

Expected: a list of ~5-10 files. `userStore.ts` should NOT be in it (already shim'd).

- [ ] **Step 2: For each file in the list, apply the de-zone edits**

Example before/after for `agentStore.ts`:

```ts
// BEFORE
import { useZoneStore } from '@/stores/zoneStore'
fetchAgents: async () => {
  const zoneId = useZoneStore.getState().activeZoneId
  if (!zoneId) return
  const agents = await client.agents.list(zoneId)
  set({ agents })
},
createAgent: async (data) => {
  const zoneId = useZoneStore.getState().activeZoneId
  if (!zoneId) return
  const agent = await client.agents.create(zoneId, data)
  // ...
}

// AFTER
fetchAgents: async () => {
  const agents = await client.agents.list()
  set({ agents })
},
createAgent: async (data) => {
  const agent = await client.agents.create(data)
  // ...
}
```

Apply this pattern to every file from Step 1. Specific files to edit (confirm against grep output):
1. `channelStore.ts` — `channels.list/create`, `dm.list`
2. `agentStore.ts` — `agents.list/create/runtimes`
3. `taskStore.ts` — already channel-scoped; just remove zone imports/selectors
4. `threadInboxStore.ts` — `threads.listAll(zoneId)` → `threads.listAll()`
5. `historyStore.ts` — `history.list(zoneId, params)` → `history.list(params)`
6. Any other file in the grep output

- [ ] **Step 3: Verify zero zone references remain in stores**

```bash
grep -nE "useZoneStore|zoneTasks|/api/zones/|zoneSlug|zoneId" src/stores/*.ts
```

Expected: no output.

- [ ] **Step 4: Update store tests that asserted on zoneId argument**

```bash
grep -lnE "zoneId|zoneStore" src/stores/*.test.ts
```

For each test file, delete the `zoneId` argument from mocked calls (e.g.,
`expect(channels.list).toHaveBeenCalledWith(zoneId, opts)` →
`expect(channels.list).toHaveBeenCalledWith(opts)`).

- [ ] **Step 5: Run store tests**

```bash
./node_modules/.bin/vitest run src/stores/
```

Expected: all previously-passing store tests still pass; new userStore tests still pass.

- [ ] **Step 6: Commit**

```bash
cd ..
git add web/src/stores/
git commit -m "web(stores): drop zoneId arg + useZoneStore imports from every surviving store"
```

---

### Task 13: Flatten router.tsx

**Files:**
- Modify: `web/src/router.tsx` (replace whole file)

- [ ] **Step 1: Replace router.tsx**

Overwrite `web/src/router.tsx`:

```tsx
import { lazy, Suspense, useEffect } from 'react'
import { Navigate, Outlet, createBrowserRouter } from 'react-router-dom'
import App from './App'
import { ChannelRoute } from './routes/ChannelRoute'
import { Skeleton } from './components/ui/Skeleton'
import { useUserStore } from '@/stores/userStore'

const AgentRoute = lazy(() =>
  import('./routes/AgentRoute').then((m) => ({ default: m.AgentRoute })),
)
const SettingsPluginsRoute = lazy(() =>
  import('./routes/SettingsPluginsRoute').then((m) => ({ default: m.SettingsPluginsRoute })),
)

function LazyFallback() {
  return (
    <div className="flex-1 flex items-center justify-center">
      <Skeleton variant="rectangle" width="100%" height="200px" />
    </div>
  )
}

function RootLayout() {
  const init = useUserStore((s) => s.init)
  useEffect(() => {
    init()
  }, [init])
  return <Outlet />
}

export const router = createBrowserRouter([
  {
    element: <RootLayout />,
    children: [
      {
        path: '/',
        element: <App />,
        children: [
          { index: true, element: <ChannelRoute /> },
          { path: 'channel/:channelId', element: <ChannelRoute /> },
          { path: 'channel/:channelId/msg/:id', element: <ChannelRoute /> },
          {
            path: 'agent/:id',
            element: (
              <Suspense fallback={<LazyFallback />}>
                <AgentRoute />
              </Suspense>
            ),
          },
          {
            path: 'settings/plugins',
            element: (
              <Suspense fallback={<LazyFallback />}>
                <SettingsPluginsRoute />
              </Suspense>
            ),
          },
          { path: '*', element: <Navigate to="/" replace /> },
        ],
      },
    ],
  },
])
```

- [ ] **Step 2: Check tsc — SettingsPluginsRoute error expected**

```bash
cd web && ./node_modules/.bin/tsc -b 2>&1 | grep "SettingsPluginsRoute"
```

Expected: 1 line about cannot find module './routes/SettingsPluginsRoute' (Phase 5 creates it).

- [ ] **Step 3: Commit**

```bash
cd ..
git add web/src/router.tsx
git commit -m "web(router): flatten — drop /z/:zoneSlug, /login, /invite; add /settings/plugins; catchall→/"
```

---

### Task 14: Rip zone branches out of App.tsx

**Files:**
- Modify: `web/src/App.tsx`
- Delete: `web/src/components/LandingPage.tsx`
- Delete: `web/src/components/landing/LandingPreview.tsx`

- [ ] **Step 1: Remove zone-only imports**

In `web/src/App.tsx`, delete these import lines:

```ts
import { useZoneStore } from '@/stores/zoneStore'
import { ZoneMembersPanel } from '@/components/sidebar/ZoneMembersPanel'
import { ZoneTaskBoard } from '@/components/tasks/ZoneTaskBoard'
import { WikiBrowser } from '@/components/wiki/WikiBrowser'
import { ProviderKeysTab } from '@/components/sidebar/ProviderKeysTab'
import { UserProfile } from '@/components/UserProfile'
import { AddDaemonDialog } from '@/components/sidebar/AddDaemonDialog'
import { CreateZoneDialog } from '@/components/sidebar/CreateZoneDialog'
import { LandingPage } from '@/components/LandingPage'
```

Drop `useParams` from `react-router-dom` if no other use remains.

- [ ] **Step 2: Remove zone state + zone-URL-fix-up effects**

Inside `AppLayout()`, delete these declarations and the entire `useEffect` blocks that depend on them:

```ts
const { zoneSlug } = useParams<{ zoneSlug?: string }>()
const fetchZones = useZoneStore((s) => s.fetchZones)
const activeZoneId = useZoneStore((s) => s.activeZoneId)
const activeZoneSlug = useZoneStore((s) => s.activeZoneSlug)
const setActiveZoneSlug = useZoneStore((s) => s.setActiveZoneSlug)
```

Plus the three effects:
- `useEffect(() => { fetchZones(); bootstrapPrefs() }, [fetchZones])`
- `useEffect(() => { ...zoneSlug→store... }, [zoneSlug, ...])`
- `useEffect(() => { ...store→URL fix-up... }, [user, activeZoneSlug, ...])`
- `useEffect(() => { if (!activeZoneId) return; ...fetchChannels/Agents/Users/Threads }, [activeZoneId])`

Replace with a single bootstrap effect:

```ts
useEffect(() => {
  bootstrapPrefs()
  useChannelStore.getState().fetchChannels()
  useChannelStore.getState().fetchDMs()
  useAgentStore.getState().fetchAgents()
  import('@/stores/threadInboxStore').then(({ useThreadInboxStore }) => {
    useThreadInboxStore.getState().fetchThreads()
  })
}, [])
```

- [ ] **Step 3: Simplify the outlet gate + remove zone_* workspacePanel branches**

In the JSX, find:

```tsx
{location.pathname.match(/^\/z\/[^/]+\/(devtools|daemons)(\/|$)/)
  ? <Outlet />
  : <div className="hidden"><Outlet /></div>}
```

Replace with:

```tsx
<Outlet />
```

Find the `workspacePanel === 'zone_members' | 'zone_tasks' | 'zone_wiki' | 'zone_credentials'` branches and delete all four. Keep `'history'`. Also update `workspacePanelStore.ts` to drop those panel values from its union type.

- [ ] **Step 4: Drop dialogs and logout button**

Delete `<AddDaemonDialog />` and `<CreateZoneDialog />` from the JSX root.

Delete the `<LogOut />` button (the logout one wired to `handleLogout`), the `handleLogout` callback, the `logout` selector (`const logout = useUserStore((s) => s.logout)`), and the `LogOut` import from `lucide-react`.

- [ ] **Step 5: Replace LandingPage gate in App()**

Find the `App()` function:

```tsx
function App() {
  const user = useUserStore((s) => s.user)
  const loading = useUserStore((s) => s.loading)
  if (loading) { return <div>...</div> }
  if (!user) { return <LandingPage /> }
  return <AppLayout />
}
```

Replace with:

```tsx
function App() {
  return <AppLayout />
}
```

- [ ] **Step 6: Delete LandingPage + LandingPreview files**

```bash
git rm web/src/components/LandingPage.tsx web/src/components/landing/LandingPreview.tsx
rmdir web/src/components/landing 2>/dev/null || true
```

- [ ] **Step 7: Verify**

```bash
cd web && ./node_modules/.bin/tsc -b 2>&1 | grep -cE "App\\.tsx|UserProfile|ZoneMembers|ZoneTaskBoard|WikiBrowser|ProviderKeysTab|AddDaemon|CreateZoneDialog|LandingPage"
```

Expected: 0 (no more zone/landing/user-profile errors in App.tsx).

- [ ] **Step 8: Commit**

```bash
cd ..
git add web/src/App.tsx web/src/stores/workspacePanelStore.ts \
        web/src/components/LandingPage.tsx web/src/components/landing
git commit -m "web(App): rip zone branches, drop LandingPage + LogOut + dialogs, single bootstrap effect"
```

---

### Task 15: De-zone SidebarTabs + sidebar consumers

**Files:**
- Modify: `web/src/components/sidebar/SidebarTabs.tsx`
- Modify: `web/src/components/sidebar/{AgentList,AgentCreateForm,AgentPanel,ChannelList,CreateChannelDialog,DMList,OpenDMDialog,ThreadInbox,SavedMessages}.tsx`

- [ ] **Step 1: Inventory remaining zone references in sidebar**

```bash
cd web
grep -lnE "useZoneStore|zoneId" src/components/sidebar/
```

Expected: ~5-8 files.

- [ ] **Step 2: Apply de-zone pattern from Task 12 to each**

Same pattern: drop `useZoneStore` import, drop `const zoneId = useZoneStore.getState().activeZoneId` line + guard, drop `zoneId` arg from client calls. Examples:

- `AgentList.tsx`: `const agents = useAgentStore((s) => s.agents)` already zone-agnostic post-Task-12; just remove any `useZoneStore` import.
- `CreateChannelDialog.tsx`: `useChannelStore.getState().createChannel(name, desc)` (no zoneId in call).
- `ChannelList.tsx`: drop zone-active gate.

- [ ] **Step 3: Trim SidebarTabs.tsx**

Open `web/src/components/sidebar/SidebarTabs.tsx`. Delete any tab buttons that mapped to deleted features (members, daemons, credentials, zone-settings). Keep: channels, agents, dms, tasks, threads (and history if present).

Add a "Plugins" link at the sidebar bottom (or wherever a settings entry lives). Use this snippet at a sensible location (e.g. inside the sidebar's footer row):

```tsx
import { Puzzle } from 'lucide-react'
import { useNavigate } from 'react-router-dom'
// ...
const navigate = useNavigate()
// ...
<button
  type="button"
  onClick={() => navigate('/settings/plugins')}
  className="p-1.5 rounded hover:bg-accent text-muted-foreground hover:text-foreground transition-colors"
  title="Plugins"
>
  <Puzzle className="h-4 w-4" />
</button>
```

- [ ] **Step 4: Verify**

```bash
./node_modules/.bin/tsc -b 2>&1 | grep -E "sidebar/" | head
./node_modules/.bin/vitest run src/components/sidebar/
```

Expected: tsc clean on sidebar; sidebar tests pass.

- [ ] **Step 5: Commit**

```bash
cd ..
git add web/src/components/sidebar/
git commit -m "web(sidebar): de-zone tabs + lists; add Plugins entry"
```

---

### Task 16: Clean shared/stores/tasks.ts + filter.ts

**Files:**
- Modify or delete: `shared/stores/tasks.ts`
- Modify: `shared/stores/filter.ts`

- [ ] **Step 1: Check consumers of shared/stores/tasks.ts**

```bash
cd web
grep -rln "@shared/stores/tasks\|shared/stores/tasks" src/ ../shared/
```

- [ ] **Step 2a (if no consumers): delete the file**

```bash
cd .. && git rm shared/stores/tasks.ts
```

- [ ] **Step 2b (if consumers exist): rewrite to channel-scoped**

```ts
// shared/stores/tasks.ts
import { create } from 'zustand'
import { tasks as tasksApi } from '@shared/api/client'
import type { Task } from '@shared/types'

interface TasksState {
  itemsByChannel: Record<string, Task[]>
  refresh: (channelId: string, status?: string) => Promise<void>
}

export const useTasksStore = create<TasksState>((set) => ({
  itemsByChannel: {},
  refresh: async (channelId, status) => {
    const rows = await tasksApi.list(channelId, status)
    set((s) => ({ itemsByChannel: { ...s.itemsByChannel, [channelId]: rows } }))
  },
}))
```

- [ ] **Step 3: Touch up filter.ts comment**

In `shared/stores/filter.ts`, find:

```ts
/** Convert a filter value to the `status` query param for zoneTasks.list. */
```

Replace with:

```ts
/** Convert a filter value to the `status` query param for tasks.list. */
```

- [ ] **Step 4: Verify shared compiles**

```bash
cd shared && ../web/node_modules/.bin/tsc --noEmit
```

Expected: exit 0.

- [ ] **Step 5: Commit**

```bash
cd ..
git add shared/stores/
git commit -m "shared(stores): drop zoneTasks dependency"
```

---

### Task 17: Phase-3 verification gate (no source changes)

- [ ] **Step 1: tsc -b in web/**

```bash
cd web && ./node_modules/.bin/tsc -b 2>&1 | tee /tmp/cocli-phase3-tsc.txt | tail -20
echo "errors: $(grep -c 'error TS' /tmp/cocli-phase3-tsc.txt)"
```

Expected: errors are ONLY about not-yet-created Phase-4/5 files:
- `routes/SettingsPluginsRoute` (Phase 5)
- `components/wizard/FirstRunWizard` (Phase 4 — already wired up if Task 18+ done; ignore here)
- `stores/wizardStore` (Phase 4)
- `stores/pluginsStore` (Phase 5)

Anything else: stop and fix before continuing.

- [ ] **Step 2: Run all existing tests**

```bash
./node_modules/.bin/vitest run --reporter=verbose 2>&1 | tail -30
```

Expected: every test passes that doesn't transitively import a not-yet-created file.

- [ ] **Step 3: ESLint baseline**

```bash
./node_modules/.bin/eslint . 2>&1 | tail -3
```

Expected: ~16 errors (residue per spec §6). Phase 7 fixes them.

- [ ] **Step 4: No commit (verification only)**

---

## Phase 4 — First-run wizard (Tasks 18-23)

### Task 18: wizardStore + tests

**Files:**
- Create: `web/src/stores/wizardStore.ts`
- Create: `web/src/stores/wizardStore.test.ts`

- [ ] **Step 1: Write the failing test**

Create `web/src/stores/wizardStore.test.ts`:

```ts
import { describe, expect, it, beforeEach, vi } from 'vitest'
import { useWizardStore } from './wizardStore'
import { useAgentStore } from './agentStore'
import { storageKey } from '@shared/brand'

const KEY_COMPLETE = storageKey('cocli-first-run-complete')
const KEY_STATE = storageKey('cocli-wizard-state')

beforeEach(() => {
  localStorage.clear()
  useWizardStore.setState({
    step: 1,
    complete: false,
    claudePath: '',
    detectedAt: null,
    draftAgent: { name: '', model: 'claude-sonnet-4-6' },
  })
  useAgentStore.setState({ agents: [] })
})

describe('useWizardStore', () => {
  it('starts at step 1 with empty state', () => {
    const s = useWizardStore.getState()
    expect(s.step).toBe(1)
    expect(s.complete).toBe(false)
    expect(s.claudePath).toBe('')
    expect(s.draftAgent).toEqual({ name: '', model: 'claude-sonnet-4-6' })
  })

  it('next() advances step and caps at 3', () => {
    useWizardStore.getState().next()
    expect(useWizardStore.getState().step).toBe(2)
    useWizardStore.getState().next()
    expect(useWizardStore.getState().step).toBe(3)
    useWizardStore.getState().next()
    expect(useWizardStore.getState().step).toBe(3)
  })

  it('back() retreats and caps at 1', () => {
    useWizardStore.setState({ step: 3 })
    useWizardStore.getState().back()
    expect(useWizardStore.getState().step).toBe(2)
    useWizardStore.getState().back()
    expect(useWizardStore.getState().step).toBe(1)
    useWizardStore.getState().back()
    expect(useWizardStore.getState().step).toBe(1)
  })

  it('setClaudePath() updates path', () => {
    useWizardStore.getState().setClaudePath('/usr/bin/claude')
    expect(useWizardStore.getState().claudePath).toBe('/usr/bin/claude')
  })

  it('detectClaudePath() sets detectedAt after a tick', async () => {
    vi.useFakeTimers()
    const p = useWizardStore.getState().detectClaudePath()
    vi.advanceTimersByTime(700)
    await p
    expect(useWizardStore.getState().detectedAt).toBeTruthy()
    vi.useRealTimers()
  })

  it('setDraftAgent() partial-merges fields', () => {
    useWizardStore.getState().setDraftAgent({ name: '@bot' })
    expect(useWizardStore.getState().draftAgent.name).toBe('@bot')
    expect(useWizardStore.getState().draftAgent.model).toBe('claude-sonnet-4-6')
  })

  it('finish() persists complete flag + pushes draft into agentStore', () => {
    useWizardStore.setState({
      draftAgent: { name: '@assistant', model: 'claude-sonnet-4-6' },
    })
    useWizardStore.getState().finish()
    expect(useWizardStore.getState().complete).toBe(true)
    expect(localStorage.getItem(KEY_COMPLETE)).toBe('true')
    const inserted = useAgentStore.getState().agents.find((a) => a.name === '@assistant')
    expect(inserted).toBeTruthy()
    expect(inserted?.model).toBe('claude-sonnet-4-6')
  })

  it('init() honors prior completion flag', () => {
    localStorage.setItem(KEY_COMPLETE, 'true')
    useWizardStore.getState().init()
    expect(useWizardStore.getState().complete).toBe(true)
  })

  it('init() restores in-progress state', () => {
    localStorage.setItem(
      KEY_STATE,
      JSON.stringify({
        step: 2,
        claudePath: '/x',
        draftAgent: { name: '@a', model: 'claude-haiku-4-5' },
      }),
    )
    useWizardStore.getState().init()
    const s = useWizardStore.getState()
    expect(s.step).toBe(2)
    expect(s.claudePath).toBe('/x')
    expect(s.draftAgent).toEqual({ name: '@a', model: 'claude-haiku-4-5' })
  })

  it('honors ?skip-wizard=1 on init', () => {
    const orig = window.location.search
    Object.defineProperty(window, 'location', {
      value: { ...window.location, search: '?skip-wizard=1' },
      writable: true,
    })
    useWizardStore.getState().init()
    expect(useWizardStore.getState().complete).toBe(true)
    Object.defineProperty(window, 'location', {
      value: { ...window.location, search: orig },
      writable: true,
    })
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd web && ./node_modules/.bin/vitest run src/stores/wizardStore.test.ts
```

Expected: FAIL (module not found).

- [ ] **Step 3: Create wizardStore.ts**

```ts
// web/src/stores/wizardStore.ts
import { create } from 'zustand'
import { useAgentStore } from './agentStore'
import { storageKey } from '@shared/brand'
import type { Agent } from '@shared/types'

export type Model = 'claude-sonnet-4-6' | 'claude-haiku-4-5' | 'claude-opus-4-7'

export interface DraftAgent {
  name: string
  model: Model
}

interface WizardState {
  step: 1 | 2 | 3
  complete: boolean
  claudePath: string
  detectedAt: string | null
  draftAgent: DraftAgent
  init: () => void
  next: () => void
  back: () => void
  setClaudePath: (p: string) => void
  detectClaudePath: () => Promise<void>
  setDraftAgent: (a: Partial<DraftAgent>) => void
  finish: () => void
}

const KEY_COMPLETE = 'cocli-first-run-complete'
const KEY_STATE = 'cocli-wizard-state'

function persistState(state: Pick<WizardState, 'step' | 'claudePath' | 'draftAgent'>) {
  localStorage.setItem(
    storageKey(KEY_STATE),
    JSON.stringify({
      step: state.step,
      claudePath: state.claudePath,
      draftAgent: state.draftAgent,
    }),
  )
}

export const useWizardStore = create<WizardState>((set, get) => ({
  step: 1,
  complete: false,
  claudePath: '',
  detectedAt: null,
  draftAgent: { name: '', model: 'claude-sonnet-4-6' },

  init: () => {
    if (new URLSearchParams(window.location.search).get('skip-wizard') === '1') {
      get().finish()
      return
    }
    if (localStorage.getItem(storageKey(KEY_COMPLETE)) === 'true') {
      set({ complete: true })
      return
    }
    const raw = localStorage.getItem(storageKey(KEY_STATE))
    if (!raw) return
    try {
      const parsed = JSON.parse(raw) as Partial<Pick<WizardState, 'step' | 'claudePath' | 'draftAgent'>>
      set({
        step: (parsed.step as 1 | 2 | 3) ?? 1,
        claudePath: parsed.claudePath ?? '',
        draftAgent: parsed.draftAgent ?? { name: '', model: 'claude-sonnet-4-6' },
      })
    } catch {
      /* corrupt JSON — start fresh */
    }
  },

  next: () => {
    const cur = get().step
    const nxt = (cur < 3 ? cur + 1 : 3) as 1 | 2 | 3
    set({ step: nxt })
    persistState(get())
  },

  back: () => {
    const cur = get().step
    const prv = (cur > 1 ? cur - 1 : 1) as 1 | 2 | 3
    set({ step: prv })
    persistState(get())
  },

  setClaudePath: (p) => {
    set({ claudePath: p })
    persistState(get())
  },

  detectClaudePath: async () => {
    await new Promise((r) => setTimeout(r, 600))
    set({ detectedAt: new Date().toISOString() })
  },

  setDraftAgent: (patch) => {
    set({ draftAgent: { ...get().draftAgent, ...patch } })
    persistState(get())
  },

  finish: () => {
    const draft = get().draftAgent
    if (draft.name) {
      const now = new Date().toISOString()
      const agent: Agent = {
        id: crypto.randomUUID(),
        name: draft.name,
        runtime: 'claude',
        model: draft.model,
        status: 'offline',
        createdAt: now,
        updatedAt: now,
      }
      useAgentStore.setState((s) => ({ agents: [...s.agents, agent] }))
    }
    localStorage.setItem(storageKey(KEY_COMPLETE), 'true')
    localStorage.removeItem(storageKey(KEY_STATE))
    set({ complete: true })
  },
}))
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
./node_modules/.bin/vitest run src/stores/wizardStore.test.ts
```

Expected: all 10 tests pass.

- [ ] **Step 5: Commit**

```bash
cd ..
git add web/src/stores/wizardStore.ts web/src/stores/wizardStore.test.ts
git commit -m "web(wizardStore): 3-step state + localStorage round-trip + ?skip-wizard URL"
```

---

### Task 19: FirstRunWizard shell + tests

**Files:**
- Create: `web/src/components/wizard/FirstRunWizard.tsx`
- Create: `web/src/components/wizard/FirstRunWizard.test.tsx`
- Create: `web/src/components/wizard/steps/{LocateClaude,CreateAgent,TryIt}Step.tsx` (stubs)

- [ ] **Step 1: Write the failing test**

```tsx
// web/src/components/wizard/FirstRunWizard.test.tsx
import { describe, expect, it, beforeEach } from 'vitest'
import { render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { FirstRunWizard } from './FirstRunWizard'
import { useWizardStore } from '@/stores/wizardStore'

beforeEach(() => {
  localStorage.clear()
  useWizardStore.setState({
    step: 1, complete: false, claudePath: '', detectedAt: null,
    draftAgent: { name: '', model: 'claude-sonnet-4-6' },
  })
})

function renderWiz() {
  return render(<MemoryRouter><FirstRunWizard /></MemoryRouter>)
}

describe('<FirstRunWizard>', () => {
  it('renders the headline "Welcome to cocli local"', () => {
    renderWiz()
    expect(screen.getByText(/Welcome to cocli local/i)).toBeInTheDocument()
  })

  it('renders 3 progress dots and highlights step 1', () => {
    renderWiz()
    const dots = screen.getAllByTestId('wizard-progress-dot')
    expect(dots).toHaveLength(3)
    expect(dots[0]).toHaveAttribute('data-active', 'true')
    expect(dots[1]).toHaveAttribute('data-active', 'false')
  })

  it('does not render when complete=true', () => {
    useWizardStore.setState({ complete: true })
    const { container } = renderWiz()
    expect(container.firstChild).toBeNull()
  })

  it('renders LocateClaudeStep at step 1', () => {
    renderWiz()
    expect(screen.getByText(/Where is your Claude CLI/i)).toBeInTheDocument()
  })

  it('renders CreateAgentStep at step 2', () => {
    useWizardStore.setState({ step: 2 })
    renderWiz()
    expect(screen.getByText(/Create your first agent/i)).toBeInTheDocument()
  })

  it('renders TryItStep at step 3', () => {
    useWizardStore.setState({ step: 3 })
    renderWiz()
    expect(screen.getByText(/You're all set/i)).toBeInTheDocument()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd web && ./node_modules/.bin/vitest run src/components/wizard/
```

Expected: FAIL.

- [ ] **Step 3: Create step stubs**

```bash
mkdir -p src/components/wizard/steps
```

`src/components/wizard/steps/LocateClaudeStep.tsx`:

```tsx
export function LocateClaudeStep() { return <div>Where is your Claude CLI?</div> }
```

`src/components/wizard/steps/CreateAgentStep.tsx`:

```tsx
export function CreateAgentStep() { return <div>Create your first agent</div> }
```

`src/components/wizard/steps/TryItStep.tsx`:

```tsx
export function TryItStep() { return <div>You're all set!</div> }
```

- [ ] **Step 4: Create FirstRunWizard.tsx**

```tsx
// web/src/components/wizard/FirstRunWizard.tsx
import { useEffect } from 'react'
import { useWizardStore } from '@/stores/wizardStore'
import { LocateClaudeStep } from './steps/LocateClaudeStep'
import { CreateAgentStep } from './steps/CreateAgentStep'
import { TryItStep } from './steps/TryItStep'
import { cn } from '@/lib/utils'

export function FirstRunWizard() {
  const step = useWizardStore((s) => s.step)
  const complete = useWizardStore((s) => s.complete)
  const init = useWizardStore((s) => s.init)

  useEffect(() => {
    init()
  }, [init])

  if (complete) return null

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 backdrop-blur-sm">
      <div className="w-[480px] max-w-[92vw] rounded-lg border bg-card shadow-2xl">
        <header className="px-6 pt-6 pb-4 border-b">
          <h1 className="text-xl font-semibold text-foreground">Welcome to cocli local</h1>
          <div className="mt-4 flex items-center gap-2" role="group" aria-label="Progress">
            {[1, 2, 3].map((n) => (
              <span
                key={n}
                data-testid="wizard-progress-dot"
                data-active={step === n ? 'true' : 'false'}
                className={cn(
                  'h-2 w-2 rounded-full transition-colors',
                  step === n ? 'bg-primary' : 'bg-muted',
                )}
              />
            ))}
          </div>
        </header>
        <div className="px-6 py-6">
          {step === 1 && <LocateClaudeStep />}
          {step === 2 && <CreateAgentStep />}
          {step === 3 && <TryItStep />}
        </div>
      </div>
    </div>
  )
}
```

- [ ] **Step 5: Run test to verify it passes**

```bash
./node_modules/.bin/vitest run src/components/wizard/
```

Expected: all 6 shell tests pass (stub step components satisfy text assertions).

- [ ] **Step 6: Commit**

```bash
cd ..
git add web/src/components/wizard/
git commit -m "web(wizard): FirstRunWizard shell + 3 step stubs"
```

---

### Task 20: LocateClaudeStep body

**Files:**
- Modify: `web/src/components/wizard/steps/LocateClaudeStep.tsx`
- Create: `web/src/components/wizard/steps/LocateClaudeStep.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
// web/src/components/wizard/steps/LocateClaudeStep.test.tsx
import { describe, expect, it, beforeEach, vi } from 'vitest'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { LocateClaudeStep } from './LocateClaudeStep'
import { useWizardStore } from '@/stores/wizardStore'

beforeEach(() => {
  localStorage.clear()
  useWizardStore.setState({
    step: 1, complete: false, claudePath: '', detectedAt: null,
    draftAgent: { name: '', model: 'claude-sonnet-4-6' },
  })
})

describe('<LocateClaudeStep>', () => {
  it('renders headline + path input + Detect button + Next', () => {
    render(<LocateClaudeStep />)
    expect(screen.getByLabelText(/path to claude/i)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /detect/i })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /next/i })).toBeInTheDocument()
  })

  it('typing updates claudePath in store', () => {
    render(<LocateClaudeStep />)
    fireEvent.change(screen.getByLabelText(/path to claude/i), {
      target: { value: '/opt/claude/bin/claude' },
    })
    expect(useWizardStore.getState().claudePath).toBe('/opt/claude/bin/claude')
  })

  it('clicking Detect shows a check on success', async () => {
    vi.useFakeTimers()
    render(<LocateClaudeStep />)
    fireEvent.click(screen.getByRole('button', { name: /detect/i }))
    vi.advanceTimersByTime(700)
    await waitFor(() => expect(screen.getByTestId('detect-success')).toBeInTheDocument())
    vi.useRealTimers()
  })

  it('Next advances to step 2 even with empty path', () => {
    render(<LocateClaudeStep />)
    fireEvent.click(screen.getByRole('button', { name: /next/i }))
    expect(useWizardStore.getState().step).toBe(2)
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd web && ./node_modules/.bin/vitest run src/components/wizard/steps/LocateClaudeStep
```

Expected: FAIL.

- [ ] **Step 3: Replace LocateClaudeStep.tsx**

```tsx
import { useState } from 'react'
import { Check, Loader2 } from 'lucide-react'
import { useWizardStore } from '@/stores/wizardStore'

export function LocateClaudeStep() {
  const claudePath = useWizardStore((s) => s.claudePath)
  const detectedAt = useWizardStore((s) => s.detectedAt)
  const setClaudePath = useWizardStore((s) => s.setClaudePath)
  const detectClaudePath = useWizardStore((s) => s.detectClaudePath)
  const next = useWizardStore((s) => s.next)
  const [detecting, setDetecting] = useState(false)

  async function handleDetect() {
    setDetecting(true)
    try {
      await detectClaudePath()
    } finally {
      setDetecting(false)
    }
  }

  return (
    <div className="space-y-5">
      <div>
        <h2 className="text-lg font-medium text-foreground">Where is your Claude CLI?</h2>
        <p className="mt-1 text-sm text-content-secondary">
          Leave blank for now — we'll auto-detect when the binary is ready in M0.0.2.
        </p>
      </div>
      <div className="space-y-2">
        <label htmlFor="claude-path" className="text-sm font-medium text-foreground">
          Path to Claude CLI
        </label>
        <div className="flex gap-2">
          <input
            id="claude-path"
            type="text"
            value={claudePath}
            onChange={(e) => setClaudePath(e.target.value)}
            placeholder="/usr/local/bin/claude"
            className="flex-1 h-9 px-3 rounded border bg-background text-sm"
          />
          <button
            type="button"
            onClick={handleDetect}
            disabled={detecting}
            className="h-9 px-3 rounded border bg-background text-sm hover:bg-accent disabled:opacity-50"
          >
            {detecting ? <Loader2 className="h-4 w-4 animate-spin" /> : 'Detect'}
          </button>
          {detectedAt && !detecting && (
            <span data-testid="detect-success" className="h-9 inline-flex items-center text-success">
              <Check className="h-4 w-4" />
            </span>
          )}
        </div>
      </div>
      <div className="flex justify-end pt-2">
        <button
          type="button"
          onClick={() => next()}
          className="h-9 px-4 rounded bg-primary text-primary-foreground text-sm hover:bg-primary/90"
        >
          Next
        </button>
      </div>
    </div>
  )
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
./node_modules/.bin/vitest run src/components/wizard/steps/LocateClaudeStep
```

Expected: all 4 tests pass.

- [ ] **Step 5: Commit**

```bash
cd ..
git add web/src/components/wizard/steps/LocateClaudeStep.tsx \
        web/src/components/wizard/steps/LocateClaudeStep.test.tsx
git commit -m "web(wizard): LocateClaudeStep body"
```

---

### Task 21: CreateAgentStep body

**Files:**
- Modify: `web/src/components/wizard/steps/CreateAgentStep.tsx`
- Create: `web/src/components/wizard/steps/CreateAgentStep.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
// web/src/components/wizard/steps/CreateAgentStep.test.tsx
import { describe, expect, it, beforeEach } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { CreateAgentStep } from './CreateAgentStep'
import { useWizardStore } from '@/stores/wizardStore'

beforeEach(() => {
  localStorage.clear()
  useWizardStore.setState({
    step: 2, complete: false, claudePath: '', detectedAt: null,
    draftAgent: { name: '', model: 'claude-sonnet-4-6' },
  })
})

describe('<CreateAgentStep>', () => {
  it('renders name + model + Back + Next', () => {
    render(<CreateAgentStep />)
    expect(screen.getByLabelText(/agent name/i)).toBeInTheDocument()
    expect(screen.getByLabelText(/model/i)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /back/i })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /next/i })).toBeInTheDocument()
  })

  it('Next is disabled when name is empty', () => {
    render(<CreateAgentStep />)
    expect(screen.getByRole('button', { name: /next/i })).toBeDisabled()
  })

  it('typing a valid name enables Next and updates store', () => {
    render(<CreateAgentStep />)
    fireEvent.change(screen.getByLabelText(/agent name/i), { target: { value: '@assistant' } })
    expect(useWizardStore.getState().draftAgent.name).toBe('@assistant')
    expect(screen.getByRole('button', { name: /next/i })).not.toBeDisabled()
  })

  it('auto-prepends @ when typing a bare name', () => {
    render(<CreateAgentStep />)
    fireEvent.change(screen.getByLabelText(/agent name/i), { target: { value: 'helper' } })
    expect(useWizardStore.getState().draftAgent.name).toBe('@helper')
  })

  it('strips invalid characters', () => {
    render(<CreateAgentStep />)
    fireEvent.change(screen.getByLabelText(/agent name/i), { target: { value: '@HAS SPACE' } })
    expect(useWizardStore.getState().draftAgent.name).toBe('@hasspace')
  })

  it('changing model updates store', () => {
    render(<CreateAgentStep />)
    fireEvent.change(screen.getByLabelText(/model/i), { target: { value: 'claude-haiku-4-5' } })
    expect(useWizardStore.getState().draftAgent.model).toBe('claude-haiku-4-5')
  })

  it('Back returns to step 1', () => {
    render(<CreateAgentStep />)
    fireEvent.click(screen.getByRole('button', { name: /back/i }))
    expect(useWizardStore.getState().step).toBe(1)
  })

  it('Next advances to step 3', () => {
    useWizardStore.setState({ draftAgent: { name: '@a', model: 'claude-sonnet-4-6' } })
    render(<CreateAgentStep />)
    fireEvent.click(screen.getByRole('button', { name: /next/i }))
    expect(useWizardStore.getState().step).toBe(3)
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd web && ./node_modules/.bin/vitest run src/components/wizard/steps/CreateAgentStep
```

Expected: FAIL.

- [ ] **Step 3: Replace CreateAgentStep.tsx**

```tsx
import { useWizardStore, type Model } from '@/stores/wizardStore'

const MODELS: { id: Model; label: string }[] = [
  { id: 'claude-sonnet-4-6', label: 'Claude Sonnet 4.6 (recommended)' },
  { id: 'claude-haiku-4-5', label: 'Claude Haiku 4.5 (fast)' },
  { id: 'claude-opus-4-7', label: 'Claude Opus 4.7 (most capable)' },
]

function normaliseName(raw: string): string {
  let s = raw.toLowerCase().replace(/[^a-z0-9@-]/g, '')
  if (s.startsWith('@')) s = '@' + s.slice(1).replace(/@/g, '')
  else s = '@' + s.replace(/@/g, '')
  return s === '@' ? '' : s
}

export function CreateAgentStep() {
  const draft = useWizardStore((s) => s.draftAgent)
  const setDraftAgent = useWizardStore((s) => s.setDraftAgent)
  const next = useWizardStore((s) => s.next)
  const back = useWizardStore((s) => s.back)
  const canAdvance = draft.name.length > 1

  return (
    <div className="space-y-5">
      <div>
        <h2 className="text-lg font-medium text-foreground">Create your first agent</h2>
        <p className="mt-1 text-sm text-content-secondary">
          This agent lives on your machine. You can change the model later.
        </p>
      </div>
      <div className="space-y-2">
        <label htmlFor="agent-name" className="text-sm font-medium text-foreground">
          Agent name
        </label>
        <input
          id="agent-name"
          type="text"
          value={draft.name}
          onChange={(e) => setDraftAgent({ name: normaliseName(e.target.value) })}
          placeholder="@assistant"
          className="w-full h-9 px-3 rounded border bg-background text-sm"
        />
      </div>
      <div className="space-y-2">
        <label htmlFor="agent-model" className="text-sm font-medium text-foreground">
          Model
        </label>
        <select
          id="agent-model"
          value={draft.model}
          onChange={(e) => setDraftAgent({ model: e.target.value as Model })}
          className="w-full h-9 px-2 rounded border bg-background text-sm"
        >
          {MODELS.map((m) => (
            <option key={m.id} value={m.id}>{m.label}</option>
          ))}
        </select>
      </div>
      <div className="flex justify-between pt-2">
        <button
          type="button"
          onClick={() => back()}
          className="h-9 px-4 rounded border bg-background text-sm hover:bg-accent"
        >
          Back
        </button>
        <button
          type="button"
          onClick={() => next()}
          disabled={!canAdvance}
          className="h-9 px-4 rounded bg-primary text-primary-foreground text-sm hover:bg-primary/90 disabled:opacity-40 disabled:cursor-not-allowed"
        >
          Next
        </button>
      </div>
    </div>
  )
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
./node_modules/.bin/vitest run src/components/wizard/steps/CreateAgentStep
```

Expected: all 8 tests pass.

- [ ] **Step 5: Commit**

```bash
cd ..
git add web/src/components/wizard/steps/CreateAgentStep.tsx \
        web/src/components/wizard/steps/CreateAgentStep.test.tsx
git commit -m "web(wizard): CreateAgentStep body — name+model+validation"
```

---

### Task 22: TryItStep body

**Files:**
- Modify: `web/src/components/wizard/steps/TryItStep.tsx`
- Create: `web/src/components/wizard/steps/TryItStep.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
// web/src/components/wizard/steps/TryItStep.test.tsx
import { describe, expect, it, beforeEach, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { MemoryRouter, useNavigate } from 'react-router-dom'
import { TryItStep } from './TryItStep'
import { useWizardStore } from '@/stores/wizardStore'

vi.mock('react-router-dom', async () => {
  const actual = await vi.importActual<typeof import('react-router-dom')>('react-router-dom')
  return { ...actual, useNavigate: vi.fn() }
})

beforeEach(() => {
  localStorage.clear()
  useWizardStore.setState({
    step: 3, complete: false, claudePath: '', detectedAt: null,
    draftAgent: { name: '@assistant', model: 'claude-sonnet-4-6' },
  })
  vi.mocked(useNavigate).mockReturnValue(vi.fn())
})

function renderStep() {
  return render(<MemoryRouter><TryItStep /></MemoryRouter>)
}

describe('<TryItStep>', () => {
  it('renders headline + Open #general + Maybe later', () => {
    renderStep()
    expect(screen.getByText(/You're all set/i)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /open #general/i })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /maybe later/i })).toBeInTheDocument()
  })

  it('Open #general calls finish() and navigates', () => {
    const navigate = vi.fn()
    vi.mocked(useNavigate).mockReturnValue(navigate)
    renderStep()
    fireEvent.click(screen.getByRole('button', { name: /open #general/i }))
    expect(useWizardStore.getState().complete).toBe(true)
    expect(navigate).toHaveBeenCalledWith('/channel/general')
  })

  it('Maybe later calls finish() but does not navigate', () => {
    const navigate = vi.fn()
    vi.mocked(useNavigate).mockReturnValue(navigate)
    renderStep()
    fireEvent.click(screen.getByRole('button', { name: /maybe later/i }))
    expect(useWizardStore.getState().complete).toBe(true)
    expect(navigate).not.toHaveBeenCalled()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd web && ./node_modules/.bin/vitest run src/components/wizard/steps/TryItStep
```

Expected: FAIL.

- [ ] **Step 3: Replace TryItStep.tsx**

```tsx
import { useNavigate } from 'react-router-dom'
import { Check } from 'lucide-react'
import { useWizardStore } from '@/stores/wizardStore'

export function TryItStep() {
  const finish = useWizardStore((s) => s.finish)
  const navigate = useNavigate()

  return (
    <div className="space-y-6">
      <div className="flex flex-col items-center text-center space-y-3">
        <div className="h-12 w-12 rounded-full bg-success/15 flex items-center justify-center">
          <Check className="h-6 w-6 text-success" />
        </div>
        <h2 className="text-lg font-medium text-foreground">You're all set!</h2>
        <p className="text-sm text-content-secondary">
          Go say hi in <span className="font-mono">#general</span>.
        </p>
      </div>
      <div className="flex flex-col gap-2 pt-2">
        <button
          type="button"
          onClick={() => {
            finish()
            navigate('/channel/general')
          }}
          className="w-full h-10 rounded bg-primary text-primary-foreground text-sm font-medium hover:bg-primary/90"
        >
          Open #general →
        </button>
        <button
          type="button"
          onClick={() => finish()}
          className="w-full h-9 rounded text-sm text-content-secondary hover:text-foreground"
        >
          Maybe later
        </button>
      </div>
    </div>
  )
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
./node_modules/.bin/vitest run src/components/wizard/steps/TryItStep
```

Expected: all 3 tests pass.

- [ ] **Step 5: Commit**

```bash
cd ..
git add web/src/components/wizard/steps/TryItStep.tsx \
        web/src/components/wizard/steps/TryItStep.test.tsx
git commit -m "web(wizard): TryItStep body — finish + navigate"
```

---

### Task 23: Mount FirstRunWizard in App.tsx

**Files:**
- Modify: `web/src/App.tsx`

- [ ] **Step 1: Add import**

At the top of `web/src/App.tsx`, after other component imports:

```ts
import { FirstRunWizard } from '@/components/wizard/FirstRunWizard'
```

- [ ] **Step 2: Mount as sibling overlay**

Wrap the `AppLayout` return in a Fragment with `<FirstRunWizard />` as the first child:

```tsx
return (
  <>
    <FirstRunWizard />
    <div className="flex h-full w-full overflow-hidden">
      {/* existing children unchanged */}
    </div>
  </>
)
```

- [ ] **Step 3: Verify tsc + tests**

```bash
cd web && ./node_modules/.bin/tsc -b 2>&1 | grep -E "App\\.tsx" | head
./node_modules/.bin/vitest run src/components/wizard/
```

Expected: tsc 0 errors on App.tsx; all wizard tests pass.

- [ ] **Step 4: Commit**

```bash
cd ..
git add web/src/App.tsx
git commit -m "web(App): mount FirstRunWizard overlay"
```

---

## Phase 5 — Plugin manager (Tasks 24-30)

Stores-only `/settings/plugins` per spec §4.

### Task 24: pluginsStore + tests

**Files:**
- Create: `web/src/stores/pluginsStore.ts`
- Create: `web/src/stores/pluginsStore.test.ts`

- [ ] **Step 1: Write the failing test**

```ts
// web/src/stores/pluginsStore.test.ts
import { describe, expect, it, beforeEach } from 'vitest'
import { usePluginsStore } from './pluginsStore'
import { storageKey } from '@shared/brand'

const KEY = storageKey('cocli-plugins')

beforeEach(() => {
  localStorage.clear()
  usePluginsStore.setState({ plugins: [] })
})

describe('usePluginsStore', () => {
  it('starts with empty plugins', () => {
    expect(usePluginsStore.getState().plugins).toEqual([])
  })

  it('list() hydrates from localStorage', async () => {
    const stored = [{
      id: 'p1', name: 'telegram-bot',
      capabilities: ['inbound-bridge'], createdAt: '2026-05-21T00:00:00Z',
      lastSeenAt: null,
    }]
    localStorage.setItem(KEY, JSON.stringify(stored))
    await usePluginsStore.getState().list()
    expect(usePluginsStore.getState().plugins).toEqual(stored)
  })

  it('register() returns plugin + token, persists, and stores plugin', async () => {
    const { plugin, token } = await usePluginsStore.getState().register(
      'telegram-bot',
      ['inbound-bridge', 'outbound-bridge'],
    )
    expect(plugin.id).toMatch(/^[0-9a-f-]{36}$/)
    expect(plugin.name).toBe('telegram-bot')
    expect(plugin.capabilities).toEqual(['inbound-bridge', 'outbound-bridge'])
    expect(plugin.lastSeenAt).toBeNull()
    expect(token).toMatch(/^[0-9a-f-]{36}$/)
    expect(usePluginsStore.getState().plugins).toHaveLength(1)
    expect(JSON.parse(localStorage.getItem(KEY)!)).toHaveLength(1)
  })

  it('revoke() removes plugin and persists', async () => {
    const { plugin } = await usePluginsStore.getState().register('a', ['inbound-bridge'])
    await usePluginsStore.getState().revoke(plugin.id)
    expect(usePluginsStore.getState().plugins).toHaveLength(0)
    expect(JSON.parse(localStorage.getItem(KEY)!)).toHaveLength(0)
  })

  it('token is NOT included in persisted localStorage payload', async () => {
    const { token } = await usePluginsStore.getState().register('a', ['inbound-bridge'])
    const raw = localStorage.getItem(KEY)!
    expect(raw).not.toContain(token)
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd web && ./node_modules/.bin/vitest run src/stores/pluginsStore.test.ts
```

Expected: FAIL (module not found).

- [ ] **Step 3: Create pluginsStore.ts**

```ts
// web/src/stores/pluginsStore.ts
import { create } from 'zustand'
import { storageKey } from '@shared/brand'
import type { Plugin, PluginCapability, PluginRegistration } from '@shared/types'

interface PluginsState {
  plugins: Plugin[]
  init: () => void
  list: () => Promise<Plugin[]>
  register: (name: string, capabilities: PluginCapability[]) => Promise<PluginRegistration>
  revoke: (id: string) => Promise<void>
}

const KEY = 'cocli-plugins'

function load(): Plugin[] {
  const raw = localStorage.getItem(storageKey(KEY))
  if (!raw) return []
  try {
    return JSON.parse(raw) as Plugin[]
  } catch {
    return []
  }
}

function persist(plugins: Plugin[]) {
  localStorage.setItem(storageKey(KEY), JSON.stringify(plugins))
}

export const usePluginsStore = create<PluginsState>((set, get) => ({
  plugins: [],

  init: () => {
    set({ plugins: load() })
  },

  list: async () => {
    const items = load()
    set({ plugins: items })
    return items
  },

  register: async (name, capabilities) => {
    const plugin: Plugin = {
      id: crypto.randomUUID(),
      name,
      capabilities,
      createdAt: new Date().toISOString(),
      lastSeenAt: null,
    }
    const token = crypto.randomUUID()
    const next = [...get().plugins, plugin]
    set({ plugins: next })
    persist(next)
    return { plugin, token }
  },

  revoke: async (id) => {
    const next = get().plugins.filter((p) => p.id !== id)
    set({ plugins: next })
    persist(next)
  },
}))
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
./node_modules/.bin/vitest run src/stores/pluginsStore.test.ts
```

Expected: all 5 tests pass.

- [ ] **Step 5: Commit**

```bash
cd ..
git add web/src/stores/pluginsStore.ts web/src/stores/pluginsStore.test.ts
git commit -m "web(pluginsStore): in-memory + localStorage; register returns plaintext token once"
```

---

### Task 25: SettingsPluginsRoute + PluginsPage shell

**Files:**
- Create: `web/src/routes/SettingsPluginsRoute.tsx`
- Create: `web/src/components/settings/plugins/PluginsPage.tsx`
- Create: `web/src/components/settings/plugins/PluginsPage.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
// web/src/components/settings/plugins/PluginsPage.test.tsx
import { describe, expect, it, beforeEach } from 'vitest'
import { render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { PluginsPage } from './PluginsPage'
import { usePluginsStore } from '@/stores/pluginsStore'

beforeEach(() => {
  localStorage.clear()
  usePluginsStore.setState({ plugins: [] })
})

function r() {
  return render(<MemoryRouter><PluginsPage /></MemoryRouter>)
}

describe('<PluginsPage>', () => {
  it('renders header Plugins + Register button', () => {
    r()
    expect(screen.getByRole('heading', { name: /plugins/i })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /register plugin/i })).toBeInTheDocument()
  })

  it('shows empty state when no plugins', () => {
    r()
    expect(screen.getByText(/No plugins yet/i)).toBeInTheDocument()
  })

  it('shows plugin rows when store has items', () => {
    usePluginsStore.setState({
      plugins: [{
        id: 'p1', name: 'telegram-bot',
        capabilities: ['inbound-bridge'],
        createdAt: '2026-05-21T00:00:00Z', lastSeenAt: null,
      }],
    })
    r()
    expect(screen.getByText('telegram-bot')).toBeInTheDocument()
    expect(screen.queryByText(/No plugins yet/i)).toBeNull()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd web && ./node_modules/.bin/vitest run src/components/settings/plugins/
```

Expected: FAIL.

- [ ] **Step 3: Create stub PluginRow + dialogs (real bodies in Tasks 26-29)**

```bash
mkdir -p src/components/settings/plugins
```

`src/components/settings/plugins/PluginRow.tsx`:

```tsx
import type { Plugin } from '@shared/types'
export function PluginRow({ plugin }: { plugin: Plugin }) {
  return <li className="py-3 border-b last:border-b-0">{plugin.name}</li>
}
```

`src/components/settings/plugins/RegisterPluginDialog.tsx`:

```tsx
export function RegisterPluginDialog({ open }: { open: boolean; onClose: () => void; onRegistered: (token: string) => void }) {
  if (!open) return null
  return <div data-testid="register-dialog" />
}
```

`src/components/settings/plugins/TokenRevealDialog.tsx`:

```tsx
export function TokenRevealDialog({ token, onClose }: { token: string | null; onClose: () => void }) {
  if (!token) return null
  return <div data-testid="token-reveal-dialog">{token}</div>
}
```

`src/components/settings/plugins/RevokeConfirmDialog.tsx`:

```tsx
import type { Plugin } from '@shared/types'
export function RevokeConfirmDialog({ plugin, onClose, onConfirm }: { plugin: Plugin | null; onClose: () => void; onConfirm: () => void }) {
  if (!plugin) return null
  return <div data-testid="revoke-dialog" />
}
```

- [ ] **Step 4: Create PluginsPage.tsx**

```tsx
// web/src/components/settings/plugins/PluginsPage.tsx
import { useEffect, useState } from 'react'
import { Puzzle } from 'lucide-react'
import { usePluginsStore } from '@/stores/pluginsStore'
import { PluginRow } from './PluginRow'
import { RegisterPluginDialog } from './RegisterPluginDialog'
import { TokenRevealDialog } from './TokenRevealDialog'
import { RevokeConfirmDialog } from './RevokeConfirmDialog'
import type { Plugin } from '@shared/types'

export function PluginsPage() {
  const plugins = usePluginsStore((s) => s.plugins)
  const init = usePluginsStore((s) => s.init)
  const revoke = usePluginsStore((s) => s.revoke)
  const [registerOpen, setRegisterOpen] = useState(false)
  const [revealToken, setRevealToken] = useState<string | null>(null)
  const [revokeTarget, setRevokeTarget] = useState<Plugin | null>(null)

  useEffect(() => { init() }, [init])

  return (
    <div className="max-w-3xl mx-auto p-6 space-y-6">
      <header className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-semibold text-foreground">Plugins</h1>
          <p className="mt-1 text-sm text-content-secondary">
            Bridge external services into your channels.
          </p>
        </div>
        <button
          type="button"
          onClick={() => setRegisterOpen(true)}
          className="h-9 px-4 rounded bg-primary text-primary-foreground text-sm font-medium hover:bg-primary/90"
        >
          Register plugin
        </button>
      </header>

      {plugins.length === 0 ? (
        <div className="flex flex-col items-center text-center py-16 space-y-3">
          <Puzzle className="h-12 w-12 text-content-secondary/50" />
          <p className="text-content-secondary">No plugins yet</p>
          <p className="text-sm text-content-secondary/70 max-w-md">
            Register one to bridge Telegram, Slack, Discord, or your own custom bridge into a cocli channel.
          </p>
        </div>
      ) : (
        <ul className="border rounded bg-card">
          {plugins.map((p) => (
            <PluginRow key={p.id} plugin={p} onRevoke={() => setRevokeTarget(p)} />
          ))}
        </ul>
      )}

      <RegisterPluginDialog
        open={registerOpen}
        onClose={() => setRegisterOpen(false)}
        onRegistered={(token) => {
          setRegisterOpen(false)
          setRevealToken(token)
        }}
      />
      <TokenRevealDialog token={revealToken} onClose={() => setRevealToken(null)} />
      <RevokeConfirmDialog
        plugin={revokeTarget}
        onClose={() => setRevokeTarget(null)}
        onConfirm={async () => {
          if (revokeTarget) await revoke(revokeTarget.id)
          setRevokeTarget(null)
        }}
      />
    </div>
  )
}
```

(Note: `PluginRow` will be updated to accept `onRevoke` in Task 26.)

- [ ] **Step 5: Create SettingsPluginsRoute.tsx**

```tsx
// web/src/routes/SettingsPluginsRoute.tsx
import { PluginsPage } from '@/components/settings/plugins/PluginsPage'

export function SettingsPluginsRoute() {
  return <PluginsPage />
}
```

- [ ] **Step 6: Update PluginRow stub to accept onRevoke prop (avoid type error)**

Replace `src/components/settings/plugins/PluginRow.tsx`:

```tsx
import type { Plugin } from '@shared/types'
export function PluginRow({ plugin }: { plugin: Plugin; onRevoke: () => void }) {
  return <li className="py-3 border-b last:border-b-0">{plugin.name}</li>
}
```

(Real body in Task 26.)

- [ ] **Step 7: Run tests**

```bash
./node_modules/.bin/vitest run src/components/settings/plugins/PluginsPage
```

Expected: all 3 tests pass.

- [ ] **Step 8: Commit**

```bash
cd ..
git add web/src/routes/SettingsPluginsRoute.tsx \
        web/src/components/settings/plugins/
git commit -m "web(plugins): SettingsPluginsRoute + PluginsPage shell with header/empty-state/list"
```

---

### Task 26: PluginRow body

**Files:**
- Modify: `web/src/components/settings/plugins/PluginRow.tsx`
- Create: `web/src/components/settings/plugins/PluginRow.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
// web/src/components/settings/plugins/PluginRow.test.tsx
import { describe, expect, it, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { PluginRow } from './PluginRow'

const sample = {
  id: 'p1',
  name: 'telegram-bot',
  capabilities: ['inbound-bridge', 'outbound-bridge'] as const,
  createdAt: '2026-05-18T00:00:00Z',
  lastSeenAt: null,
}

describe('<PluginRow>', () => {
  it('renders plugin name + capability badges', () => {
    render(<ul><PluginRow plugin={{ ...sample, capabilities: [...sample.capabilities] }} onRevoke={() => {}} /></ul>)
    expect(screen.getByText('telegram-bot')).toBeInTheDocument()
    expect(screen.getByText('inbound-bridge')).toBeInTheDocument()
    expect(screen.getByText('outbound-bridge')).toBeInTheDocument()
  })

  it('renders "Last seen: never" when lastSeenAt is null', () => {
    render(<ul><PluginRow plugin={{ ...sample, capabilities: [...sample.capabilities] }} onRevoke={() => {}} /></ul>)
    expect(screen.getByText(/last seen: never/i)).toBeInTheDocument()
  })

  it('trash button calls onRevoke', () => {
    const onRevoke = vi.fn()
    render(<ul><PluginRow plugin={{ ...sample, capabilities: [...sample.capabilities] }} onRevoke={onRevoke} /></ul>)
    fireEvent.click(screen.getByRole('button', { name: /revoke/i }))
    expect(onRevoke).toHaveBeenCalledOnce()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd web && ./node_modules/.bin/vitest run src/components/settings/plugins/PluginRow
```

Expected: FAIL.

- [ ] **Step 3: Replace PluginRow.tsx**

```tsx
// web/src/components/settings/plugins/PluginRow.tsx
import { Trash2 } from 'lucide-react'
import type { Plugin } from '@shared/types'

const capColor: Record<string, string> = {
  'inbound-bridge': 'bg-success/15 text-success-emphasis',
  'outbound-bridge': 'bg-info/15 text-info-emphasis',
}

export function PluginRow({ plugin, onRevoke }: { plugin: Plugin; onRevoke: () => void }) {
  return (
    <li className="flex items-center justify-between px-4 py-3 border-b last:border-b-0">
      <div className="flex flex-col gap-1 min-w-0">
        <span className="font-mono text-sm text-foreground truncate">{plugin.name}</span>
        <div className="flex flex-wrap items-center gap-1.5 text-xs">
          {plugin.capabilities.map((c) => (
            <span
              key={c}
              className={`px-1.5 py-0.5 rounded ${capColor[c] ?? 'bg-muted text-muted-foreground'}`}
            >
              {c}
            </span>
          ))}
          <span className="text-content-secondary">
            • Last seen: {plugin.lastSeenAt ?? 'never'}
          </span>
        </div>
      </div>
      <button
        type="button"
        onClick={onRevoke}
        title="Revoke plugin"
        aria-label="revoke"
        className="p-2 rounded hover:bg-destructive/10 text-content-secondary hover:text-destructive transition-colors"
      >
        <Trash2 className="h-4 w-4" />
      </button>
    </li>
  )
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
./node_modules/.bin/vitest run src/components/settings/plugins/PluginRow
```

Expected: all 3 tests pass.

- [ ] **Step 5: Commit**

```bash
cd ..
git add web/src/components/settings/plugins/PluginRow.tsx \
        web/src/components/settings/plugins/PluginRow.test.tsx
git commit -m "web(plugins): PluginRow body — name + cap badges + revoke button"
```

---

### Task 27: RegisterPluginDialog body

**Files:**
- Modify: `web/src/components/settings/plugins/RegisterPluginDialog.tsx`
- Create: `web/src/components/settings/plugins/RegisterPluginDialog.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
// web/src/components/settings/plugins/RegisterPluginDialog.test.tsx
import { describe, expect, it, beforeEach, vi } from 'vitest'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { RegisterPluginDialog } from './RegisterPluginDialog'
import { usePluginsStore } from '@/stores/pluginsStore'

beforeEach(() => {
  localStorage.clear()
  usePluginsStore.setState({ plugins: [] })
})

describe('<RegisterPluginDialog>', () => {
  it('renders nothing when open=false', () => {
    const { container } = render(
      <RegisterPluginDialog open={false} onClose={() => {}} onRegistered={() => {}} />,
    )
    expect(container.firstChild).toBeNull()
  })

  it('renders name input + both capability checkboxes + Register button', () => {
    render(<RegisterPluginDialog open={true} onClose={() => {}} onRegistered={() => {}} />)
    expect(screen.getByLabelText(/plugin name/i)).toBeInTheDocument()
    expect(screen.getByLabelText(/inbound-bridge/i)).toBeInTheDocument()
    expect(screen.getByLabelText(/outbound-bridge/i)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /register/i })).toBeInTheDocument()
  })

  it('Register is disabled until name + at least one capability', () => {
    render(<RegisterPluginDialog open={true} onClose={() => {}} onRegistered={() => {}} />)
    const submit = screen.getByRole('button', { name: /register/i })
    expect(submit).toBeDisabled()

    fireEvent.change(screen.getByLabelText(/plugin name/i), { target: { value: 'tg' } })
    expect(submit).toBeDisabled() // still needs capability

    fireEvent.click(screen.getByLabelText(/inbound-bridge/i))
    expect(submit).not.toBeDisabled()
  })

  it('submitting calls store.register and onRegistered with the plaintext token', async () => {
    const onRegistered = vi.fn()
    render(<RegisterPluginDialog open={true} onClose={() => {}} onRegistered={onRegistered} />)
    fireEvent.change(screen.getByLabelText(/plugin name/i), { target: { value: 'telegram-bot' } })
    fireEvent.click(screen.getByLabelText(/inbound-bridge/i))
    fireEvent.click(screen.getByRole('button', { name: /register/i }))
    await waitFor(() => expect(onRegistered).toHaveBeenCalled())
    const [token] = onRegistered.mock.calls[0]
    expect(token).toMatch(/^[0-9a-f-]{36}$/)
    expect(usePluginsStore.getState().plugins).toHaveLength(1)
  })

  it('Cancel calls onClose without registering', () => {
    const onClose = vi.fn()
    render(<RegisterPluginDialog open={true} onClose={onClose} onRegistered={() => {}} />)
    fireEvent.click(screen.getByRole('button', { name: /cancel/i }))
    expect(onClose).toHaveBeenCalledOnce()
    expect(usePluginsStore.getState().plugins).toHaveLength(0)
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd web && ./node_modules/.bin/vitest run src/components/settings/plugins/RegisterPluginDialog
```

Expected: FAIL.

- [ ] **Step 3: Replace RegisterPluginDialog.tsx**

```tsx
// web/src/components/settings/plugins/RegisterPluginDialog.tsx
import { useState } from 'react'
import { usePluginsStore } from '@/stores/pluginsStore'
import type { PluginCapability } from '@shared/types'

const CAPS: PluginCapability[] = ['inbound-bridge', 'outbound-bridge']

function normaliseName(raw: string): string {
  return raw.toLowerCase().replace(/[^a-z0-9-]/g, '').slice(0, 64)
}

export function RegisterPluginDialog({
  open,
  onClose,
  onRegistered,
}: {
  open: boolean
  onClose: () => void
  onRegistered: (token: string) => void
}) {
  const register = usePluginsStore((s) => s.register)
  const [name, setName] = useState('')
  const [selected, setSelected] = useState<PluginCapability[]>([])
  const [submitting, setSubmitting] = useState(false)

  if (!open) return null

  const canSubmit = name.length > 0 && selected.length > 0 && !submitting

  function toggle(cap: PluginCapability) {
    setSelected((s) => (s.includes(cap) ? s.filter((x) => x !== cap) : [...s, cap]))
  }

  async function handleSubmit() {
    if (!canSubmit) return
    setSubmitting(true)
    try {
      const { token } = await register(name, selected)
      setName('')
      setSelected([])
      onRegistered(token)
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-background/70 backdrop-blur-sm">
      <div role="dialog" aria-modal="true" className="w-[440px] max-w-[92vw] rounded-lg border bg-card shadow-2xl">
        <header className="px-5 pt-5 pb-3 border-b">
          <h2 className="text-base font-semibold text-foreground">Register plugin</h2>
        </header>
        <div className="px-5 py-4 space-y-4">
          <div className="space-y-2">
            <label htmlFor="plugin-name" className="text-sm font-medium text-foreground">
              Plugin name
            </label>
            <input
              id="plugin-name"
              type="text"
              value={name}
              onChange={(e) => setName(normaliseName(e.target.value))}
              placeholder="telegram-bot"
              className="w-full h-9 px-3 rounded border bg-background text-sm font-mono"
            />
          </div>
          <fieldset className="space-y-2">
            <legend className="text-sm font-medium text-foreground">Capabilities</legend>
            {CAPS.map((cap) => (
              <label key={cap} className="flex items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  checked={selected.includes(cap)}
                  onChange={() => toggle(cap)}
                />
                <span>{cap}</span>
              </label>
            ))}
          </fieldset>
        </div>
        <footer className="px-5 py-3 border-t flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            className="h-9 px-4 rounded border bg-background text-sm hover:bg-accent"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={handleSubmit}
            disabled={!canSubmit}
            className="h-9 px-4 rounded bg-primary text-primary-foreground text-sm hover:bg-primary/90 disabled:opacity-40 disabled:cursor-not-allowed"
          >
            Register
          </button>
        </footer>
      </div>
    </div>
  )
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
./node_modules/.bin/vitest run src/components/settings/plugins/RegisterPluginDialog
```

Expected: all 5 tests pass.

- [ ] **Step 5: Commit**

```bash
cd ..
git add web/src/components/settings/plugins/RegisterPluginDialog.tsx \
        web/src/components/settings/plugins/RegisterPluginDialog.test.tsx
git commit -m "web(plugins): RegisterPluginDialog body"
```

---

### Task 28: TokenRevealDialog body

**Files:**
- Modify: `web/src/components/settings/plugins/TokenRevealDialog.tsx`
- Create: `web/src/components/settings/plugins/TokenRevealDialog.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
// web/src/components/settings/plugins/TokenRevealDialog.test.tsx
import { describe, expect, it, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { TokenRevealDialog } from './TokenRevealDialog'

describe('<TokenRevealDialog>', () => {
  it('renders nothing when token=null', () => {
    const { container } = render(<TokenRevealDialog token={null} onClose={() => {}} />)
    expect(container.firstChild).toBeNull()
  })

  it('renders the token in a monospace box + Copy button + warning + Done', () => {
    render(<TokenRevealDialog token="abc-123-def" onClose={() => {}} />)
    expect(screen.getByText('abc-123-def')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /copy/i })).toBeInTheDocument()
    expect(screen.getByText(/won't be shown again/i)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /i've saved it/i })).toBeInTheDocument()
  })

  it('Copy button writes the token to the clipboard', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined)
    Object.assign(navigator, { clipboard: { writeText } })
    render(<TokenRevealDialog token="abc-123" onClose={() => {}} />)
    fireEvent.click(screen.getByRole('button', { name: /copy/i }))
    expect(writeText).toHaveBeenCalledWith('abc-123')
  })

  it('I\'ve saved it button calls onClose', () => {
    const onClose = vi.fn()
    render(<TokenRevealDialog token="abc" onClose={onClose} />)
    fireEvent.click(screen.getByRole('button', { name: /i've saved it/i }))
    expect(onClose).toHaveBeenCalledOnce()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd web && ./node_modules/.bin/vitest run src/components/settings/plugins/TokenRevealDialog
```

Expected: FAIL.

- [ ] **Step 3: Replace TokenRevealDialog.tsx**

```tsx
// web/src/components/settings/plugins/TokenRevealDialog.tsx
import { Copy } from 'lucide-react'

export function TokenRevealDialog({
  token,
  onClose,
}: {
  token: string | null
  onClose: () => void
}) {
  if (!token) return null
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-background/70 backdrop-blur-sm">
      <div role="dialog" aria-modal="true" className="w-[480px] max-w-[92vw] rounded-lg border bg-card shadow-2xl">
        <header className="px-5 pt-5 pb-3 border-b">
          <h2 className="text-base font-semibold text-foreground">Plugin registered</h2>
        </header>
        <div className="px-5 py-4 space-y-4">
          <div className="rounded border bg-muted p-3 font-mono text-sm break-all">
            {token}
          </div>
          <button
            type="button"
            onClick={() => navigator.clipboard?.writeText(token)}
            className="inline-flex items-center gap-1.5 h-8 px-3 rounded border bg-background text-sm hover:bg-accent"
          >
            <Copy className="h-3.5 w-3.5" /> Copy
          </button>
          <p className="text-sm text-warning-emphasis bg-warning/10 border border-warning/30 rounded p-3">
            Save this token — it won't be shown again. If you lose it, revoke the plugin and register a new one.
          </p>
        </div>
        <footer className="px-5 py-3 border-t flex justify-end">
          <button
            type="button"
            onClick={onClose}
            className="h-9 px-4 rounded bg-primary text-primary-foreground text-sm hover:bg-primary/90"
          >
            I've saved it
          </button>
        </footer>
      </div>
    </div>
  )
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
./node_modules/.bin/vitest run src/components/settings/plugins/TokenRevealDialog
```

Expected: all 4 tests pass.

- [ ] **Step 5: Commit**

```bash
cd ..
git add web/src/components/settings/plugins/TokenRevealDialog.tsx \
        web/src/components/settings/plugins/TokenRevealDialog.test.tsx
git commit -m "web(plugins): TokenRevealDialog body"
```

---

### Task 29: RevokeConfirmDialog body

**Files:**
- Modify: `web/src/components/settings/plugins/RevokeConfirmDialog.tsx`
- Create: `web/src/components/settings/plugins/RevokeConfirmDialog.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
// web/src/components/settings/plugins/RevokeConfirmDialog.test.tsx
import { describe, expect, it, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { RevokeConfirmDialog } from './RevokeConfirmDialog'

const sample = {
  id: 'p1', name: 'telegram-bot',
  capabilities: ['inbound-bridge' as const],
  createdAt: '2026-05-21T00:00:00Z', lastSeenAt: null,
}

describe('<RevokeConfirmDialog>', () => {
  it('renders nothing when plugin=null', () => {
    const { container } = render(<RevokeConfirmDialog plugin={null} onClose={() => {}} onConfirm={() => {}} />)
    expect(container.firstChild).toBeNull()
  })

  it('renders body with plugin name and both buttons', () => {
    render(<RevokeConfirmDialog plugin={{ ...sample, capabilities: [...sample.capabilities] }} onClose={() => {}} onConfirm={() => {}} />)
    expect(screen.getByText(/telegram-bot/i)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /cancel/i })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /revoke/i })).toBeInTheDocument()
  })

  it('Revoke calls onConfirm; Cancel calls onClose', () => {
    const onConfirm = vi.fn()
    const onClose = vi.fn()
    render(<RevokeConfirmDialog plugin={{ ...sample, capabilities: [...sample.capabilities] }} onClose={onClose} onConfirm={onConfirm} />)
    fireEvent.click(screen.getByRole('button', { name: /revoke/i }))
    expect(onConfirm).toHaveBeenCalledOnce()
    fireEvent.click(screen.getByRole('button', { name: /cancel/i }))
    expect(onClose).toHaveBeenCalledOnce()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd web && ./node_modules/.bin/vitest run src/components/settings/plugins/RevokeConfirmDialog
```

Expected: FAIL.

- [ ] **Step 3: Replace RevokeConfirmDialog.tsx**

```tsx
// web/src/components/settings/plugins/RevokeConfirmDialog.tsx
import type { Plugin } from '@shared/types'

export function RevokeConfirmDialog({
  plugin,
  onClose,
  onConfirm,
}: {
  plugin: Plugin | null
  onClose: () => void
  onConfirm: () => void
}) {
  if (!plugin) return null
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-background/70 backdrop-blur-sm">
      <div role="dialog" aria-modal="true" className="w-[400px] max-w-[92vw] rounded-lg border bg-card shadow-2xl">
        <div className="px-5 py-5 space-y-3">
          <h2 className="text-base font-semibold text-foreground">Revoke plugin</h2>
          <p className="text-sm text-content-secondary">
            Revoke <span className="font-mono">{plugin.name}</span>? Connected bridges will disconnect.
          </p>
        </div>
        <footer className="px-5 py-3 border-t flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            className="h-9 px-4 rounded border bg-background text-sm hover:bg-accent"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={onConfirm}
            className="h-9 px-4 rounded bg-destructive text-destructive-foreground text-sm hover:bg-destructive/90"
          >
            Revoke
          </button>
        </footer>
      </div>
    </div>
  )
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
./node_modules/.bin/vitest run src/components/settings/plugins/RevokeConfirmDialog
```

Expected: all 3 tests pass.

- [ ] **Step 5: Commit**

```bash
cd ..
git add web/src/components/settings/plugins/RevokeConfirmDialog.tsx \
        web/src/components/settings/plugins/RevokeConfirmDialog.test.tsx
git commit -m "web(plugins): RevokeConfirmDialog body"
```

---

### Task 30: Plugin manager integration test (full register → reveal → revoke flow)

**Files:**
- Create: `web/src/components/settings/plugins/PluginsPage.integration.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
// web/src/components/settings/plugins/PluginsPage.integration.test.tsx
import { describe, expect, it, beforeEach } from 'vitest'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { PluginsPage } from './PluginsPage'
import { usePluginsStore } from '@/stores/pluginsStore'

beforeEach(() => {
  localStorage.clear()
  usePluginsStore.setState({ plugins: [] })
})

describe('PluginsPage integration', () => {
  it('full register → reveal → close → revoke flow', async () => {
    render(<MemoryRouter><PluginsPage /></MemoryRouter>)

    expect(screen.getByText(/No plugins yet/i)).toBeInTheDocument()

    // Register
    fireEvent.click(screen.getByRole('button', { name: /register plugin/i }))
    fireEvent.change(screen.getByLabelText(/plugin name/i), { target: { value: 'telegram-bot' } })
    fireEvent.click(screen.getByLabelText(/inbound-bridge/i))
    fireEvent.click(screen.getByRole('button', { name: /register/i }))

    // Token reveal
    const reveal = await screen.findByText(/won't be shown again/i)
    expect(reveal).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: /i've saved it/i }))

    // Row appears
    await waitFor(() => expect(screen.getByText('telegram-bot')).toBeInTheDocument())
    expect(usePluginsStore.getState().plugins).toHaveLength(1)

    // Revoke
    fireEvent.click(screen.getByRole('button', { name: /revoke/i }))
    fireEvent.click(screen.getAllByRole('button', { name: /revoke/i }).at(-1)!) // confirm in dialog

    await waitFor(() => expect(screen.queryByText('telegram-bot')).toBeNull())
    expect(usePluginsStore.getState().plugins).toHaveLength(0)
  })
})
```

- [ ] **Step 2: Run test to verify it passes (no source change needed)**

```bash
cd web && ./node_modules/.bin/vitest run src/components/settings/plugins/PluginsPage.integration
```

Expected: PASS.

- [ ] **Step 3: Commit**

```bash
cd ..
git add web/src/components/settings/plugins/PluginsPage.integration.test.tsx
git commit -m "web(plugins): integration test for full register/reveal/revoke flow"
```

---

## Phase 6 — Branding (Tasks 31-32)

### Task 31: BrandLogo wordmark

**Files:**
- Modify: `web/src/components/BrandLogo.tsx`

- [ ] **Step 1: Read current BrandLogo + write the failing test**

```bash
cd web && cat src/components/BrandLogo.tsx
```

Create `web/src/components/BrandLogo.test.tsx`:

```tsx
import { describe, expect, it } from 'vitest'
import { render, screen } from '@testing-library/react'
import { BrandLogo } from './BrandLogo'

describe('<BrandLogo>', () => {
  it('renders "cocli" text', () => {
    render(<BrandLogo />)
    expect(screen.getByText('cocli')).toBeInTheDocument()
  })

  it('respects size prop', () => {
    const { rerender } = render(<BrandLogo size="sm" />)
    expect(screen.getByText('cocli').className).toMatch(/text-/)
    rerender(<BrandLogo size="lg" />)
    expect(screen.getByText('cocli').className).toMatch(/text-/)
  })
})
```

- [ ] **Step 2: Run test to verify it fails (depending on current shape)**

```bash
./node_modules/.bin/vitest run src/components/BrandLogo
```

Expected: probably FAIL (current logo may render brand from `@/brand`).

- [ ] **Step 3: Replace BrandLogo.tsx**

```tsx
// web/src/components/BrandLogo.tsx
import { cn } from '@/lib/utils'

type Size = 'sm' | 'md' | 'lg'

const sizeMap: Record<Size, string> = {
  sm: 'text-sm',
  md: 'text-base',
  lg: 'text-lg',
}

export function BrandLogo({
  size = 'md',
  textClassName,
}: {
  size?: Size
  textClassName?: string
}) {
  return (
    <span
      className={cn(
        'font-sans font-medium tracking-tight text-foreground select-none',
        sizeMap[size],
        textClassName,
      )}
    >
      cocli
    </span>
  )
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
./node_modules/.bin/vitest run src/components/BrandLogo
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cd ..
git add web/src/components/BrandLogo.tsx web/src/components/BrandLogo.test.tsx
git commit -m "web(branding): BrandLogo as plain Inter wordmark"
```

---

### Task 32: Favicon + index.html title + meta

**Files:**
- Create: `web/public/favicon.svg`
- Modify: `web/index.html`
- Delete: any legacy `web/public/favicon.ico` / `favicon.png` / `vite.svg`

- [ ] **Step 1: Create favicon.svg**

```bash
cd web
cat > public/favicon.svg <<'EOF'
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" fill="currentColor">
  <text x="16" y="22" text-anchor="middle" font-family="Inter, -apple-system, system-ui, sans-serif" font-weight="500" font-size="22">c</text>
</svg>
EOF
```

- [ ] **Step 2: Remove legacy favicons if present**

```bash
ls public/ | grep -E "favicon\\.|vite\\.svg" | while read f; do git rm "public/$f"; done
```

(`git rm` will fail silently if nothing matches — fine.)

- [ ] **Step 3: Update index.html**

Open `web/index.html`. Replace `<title>...</title>` with:

```html
<title>cocli local</title>
```

Just after the title (or replacing existing meta description), add:

```html
<meta name="description" content="Multi-agent Claude on your machine." />
```

Replace (or add) the favicon link tag:

```html
<link rel="icon" type="image/svg+xml" href="/favicon.svg" />
```

Remove any `<link rel="icon" href="/favicon.ico">` or `/vite.svg` references.

- [ ] **Step 4: Verify**

```bash
grep -E "favicon|title|description" index.html
```

Expected: only the three new lines (svg favicon, "cocli local" title, "Multi-agent Claude" meta).

- [ ] **Step 5: Commit**

```bash
cd ..
git add web/index.html web/public/favicon.svg web/public
git commit -m "web(branding): favicon.svg + title 'cocli local' + meta description"
```

---

## Phase 7 — ESLint zero (Task 33)

### Task 33: Fix the 16 residual ESLint errors

**Files:**
- Modify: `web/src/components/chat/ChannelMemoryPanel.test.tsx`
- Modify: `web/src/components/sidebar/CreateChannelDialog.test.tsx`
- Modify: `web/src/stores/memoryStore.test.ts`
- Modify: `web/src/theme/__tests__/useTheme.test.tsx`

- [ ] **Step 1: Capture pre-fix count**

```bash
cd web && ./node_modules/.bin/eslint . 2>&1 | tail -1
```

Expected: `✖ 16 problems (15 errors, 1 warning)` ± a couple (some `zone/*` errors may have already disappeared via Phase 1 deletes; ones in surviving files remain).

- [ ] **Step 2: Fix `no-explicit-any` in test files**

For each file in the list, replace every `as any` with either a minimal `unknown` cast (`as unknown as <ConcreteType>`) or a small inline type. Example:

Before:

```ts
const fake = makeStore({ channels: [] } as any)
```

After:

```ts
const fake = makeStore({ channels: [] } as unknown as ChannelState)
```

Or, when defining mock objects:

Before:

```ts
const mockClient: any = { messages: { list: vi.fn() } }
```

After:

```ts
type MockClient = { messages: { list: ReturnType<typeof vi.fn> } }
const mockClient: MockClient = { messages: { list: vi.fn() } }
```

Walk each file, replace every `: any` and every `as any`.

- [ ] **Step 3: Fix `no-empty` in useTheme.test.tsx**

Find the empty arrow function (likely on line 12 of `useTheme.test.tsx`):

```ts
() => {}
```

Replace with:

```ts
() => { /* intentionally empty: spy receives no payload */ }
```

- [ ] **Step 4: Verify zero errors**

```bash
./node_modules/.bin/eslint . 2>&1 | tail -3
```

Expected: `✖ 0 problems` (or no output if `eslint` exits silently on success).

- [ ] **Step 5: Run full test suite**

```bash
./node_modules/.bin/vitest run --reporter=dot 2>&1 | tail -5
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
cd ..
git add web/src/components/chat/ChannelMemoryPanel.test.tsx \
        web/src/components/sidebar/CreateChannelDialog.test.tsx \
        web/src/stores/memoryStore.test.ts \
        web/src/theme/__tests__/useTheme.test.tsx
git commit -m "web(eslint): zero residual errors — drop any/empty-block in test files"
```

---

## Phase 8 — i18n keys + acceptance + final commit (Tasks 34-36)

### Task 34: i18n keys for wizard + plugins

**Files:**
- Modify: `web/src/i18n/locales/en.json` (or wherever en strings live)
- Modify: `web/src/i18n/locales/zh.json` (or zh equivalent)

- [ ] **Step 1: Discover the i18n file layout**

```bash
cd web && find src/i18n -type f
```

Expected: a couple of JSON files (en + zh) and a setup `.ts`. Note the exact paths.

- [ ] **Step 2: Add wizard.* + plugins.* + sidebar.plugins keys to en.json**

Add under the top-level object (preserve existing keys):

```json
{
  "wizard": {
    "title": "Welcome to cocli local",
    "step.locate.heading": "Where is your Claude CLI?",
    "step.locate.help": "Leave blank for now — we'll auto-detect when the binary is ready in M0.0.2.",
    "step.locate.label": "Path to Claude CLI",
    "step.locate.placeholder": "/usr/local/bin/claude",
    "step.locate.detect": "Detect",
    "step.create.heading": "Create your first agent",
    "step.create.help": "This agent lives on your machine. You can change the model later.",
    "step.create.nameLabel": "Agent name",
    "step.create.modelLabel": "Model",
    "step.try.heading": "You're all set!",
    "step.try.body": "Go say hi in #general.",
    "step.try.cta": "Open #general →",
    "step.try.later": "Maybe later",
    "common.back": "Back",
    "common.next": "Next"
  },
  "plugins": {
    "title": "Plugins",
    "subtitle": "Bridge external services into your channels.",
    "register": "Register plugin",
    "empty.heading": "No plugins yet",
    "empty.body": "Register one to bridge Telegram, Slack, Discord, or your own custom bridge into a cocli channel.",
    "lastSeen.never": "never",
    "dialog.register.title": "Register plugin",
    "dialog.register.nameLabel": "Plugin name",
    "dialog.register.capabilities": "Capabilities",
    "dialog.register.cancel": "Cancel",
    "dialog.register.submit": "Register",
    "dialog.reveal.title": "Plugin registered",
    "dialog.reveal.copy": "Copy",
    "dialog.reveal.warning": "Save this token — it won't be shown again. If you lose it, revoke the plugin and register a new one.",
    "dialog.reveal.done": "I've saved it",
    "dialog.revoke.title": "Revoke plugin",
    "dialog.revoke.body": "Revoke {{name}}? Connected bridges will disconnect.",
    "dialog.revoke.cancel": "Cancel",
    "dialog.revoke.confirm": "Revoke",
    "row.revoke": "Revoke plugin"
  },
  "sidebar.plugins": "Plugins"
}
```

(Adjust to nested structure if existing keys use nested objects vs dotted strings — match prevailing style.)

- [ ] **Step 3: Mirror keys in zh.json**

Same shape, Chinese translations:

```json
{
  "wizard": {
    "title": "欢迎使用 cocli local",
    "step.locate.heading": "Claude CLI 在哪里?",
    "step.locate.help": "先空着也行 — M0.0.2 二进制就绪后会自动检测。",
    "step.locate.label": "Claude CLI 路径",
    "step.locate.placeholder": "/usr/local/bin/claude",
    "step.locate.detect": "检测",
    "step.create.heading": "创建你的第一个 agent",
    "step.create.help": "这个 agent 跑在你本机。模型可以稍后改。",
    "step.create.nameLabel": "Agent 名字",
    "step.create.modelLabel": "Model",
    "step.try.heading": "全部就绪!",
    "step.try.body": "去 #general 打个招呼吧。",
    "step.try.cta": "打开 #general →",
    "step.try.later": "稍后",
    "common.back": "返回",
    "common.next": "下一步"
  },
  "plugins": {
    "title": "插件",
    "subtitle": "把外部服务桥接到 cocli channel。",
    "register": "注册插件",
    "empty.heading": "还没有插件",
    "empty.body": "注册一个,把 Telegram、Slack、Discord 或自定义桥接对接到 cocli channel。",
    "lastSeen.never": "从未",
    "dialog.register.title": "注册插件",
    "dialog.register.nameLabel": "插件名字",
    "dialog.register.capabilities": "能力",
    "dialog.register.cancel": "取消",
    "dialog.register.submit": "注册",
    "dialog.reveal.title": "插件已注册",
    "dialog.reveal.copy": "复制",
    "dialog.reveal.warning": "请保存好这个 token — 不会再次显示。丢了就 revoke 重新注册一个。",
    "dialog.reveal.done": "我已保存",
    "dialog.revoke.title": "撤销插件",
    "dialog.revoke.body": "确定撤销 {{name}}? 已连接的桥接会断开。",
    "dialog.revoke.cancel": "取消",
    "dialog.revoke.confirm": "撤销"
  },
  "sidebar.plugins": "插件"
}
```

- [ ] **Step 4 (optional, scope-bounded): wire t() into the new components**

Hardcoded English strings in wizard + plugin components can stay literal for v0 (spec §11.2 lists i18n audit as M0.0.4 polish work). This task only adds the keys to en/zh json — wiring is deferred. Skip if no time.

- [ ] **Step 5: Verify JSON is valid**

```bash
cd web && node -e "JSON.parse(require('fs').readFileSync('src/i18n/locales/en.json','utf8'))" && \
node -e "JSON.parse(require('fs').readFileSync('src/i18n/locales/zh.json','utf8'))" && \
echo OK
```

Expected: `OK`.

- [ ] **Step 6: Commit**

```bash
cd ..
git add web/src/i18n/
git commit -m "web(i18n): add wizard.* + plugins.* + sidebar.plugins keys (en + zh)"
```

---

### Task 35: Acceptance — run spec §10 checklist + grep guards

This task is manual smoke; no source changes. **DO NOT skip** — it's the human-eye gate before declaring the slice done.

- [ ] **Step 1: Install + lint + test + build**

```bash
cd web
npm install
./node_modules/.bin/eslint .                     # MUST exit 0
./node_modules/.bin/vitest run --reporter=dot    # MUST pass
./node_modules/.bin/tsc -b                       # MUST exit 0
./node_modules/.bin/vite build                   # MUST succeed
```

Expected: all four commands exit 0.

- [ ] **Step 2: Grep guards — no surviving zone/SaaS references**

```bash
cd ..
git grep -nE 'zone[A-Z]|/zones/|zoneId|zoneSlug|chatrsCredentials|LoginPage|InviteSignup|ProviderKey|zoneAdmin|zoneTaskBoard|SkillsLibrary|ZoneMembers|ZoneSwitcher|AddDaemon|CreateZone|CreateKey|UserList|X-API-Key' web/src shared/
```

Expected: zero output. Any hit must be fixed before commit.

- [ ] **Step 3: Manual browser smoke**

```bash
cd web
cp .env.local.example .env.local
./node_modules/.bin/vite dev &
echo "open http://localhost:5173 and walk the checklist below"
```

Walk in browser:

1. App loads at `localhost:5173`.
2. Wizard overlay appears with "Welcome to cocli local".
3. Click "Detect" → green check after ~0.6s.
4. Click "Next" → CreateAgentStep.
5. Type `@assistant`, leave model at Sonnet, click Next.
6. Click "Open #general →" → URL is `/channel/general`, sidebar shows "@assistant" agent.
7. Click the Puzzle icon in sidebar → `/settings/plugins` route.
8. Empty state visible.
9. Register `telegram-bot` with `inbound-bridge` checked → token reveal modal shows UUID.
10. Click "Copy" → no error (toast or silent).
11. Click "I've saved it" → row visible.
12. Click trash icon → confirm → row vanishes.
13. Refresh page → still on `/settings/plugins`, but plugin re-appears (revoke survived because it was persisted before refresh — verify the *register-and-leave-without-revoke* case in a fresh session).
14. Fresh session: `localStorage.clear()` + refresh → wizard reappears.

Kill the dev server when done:

```bash
pkill -f 'vite dev' || true
```

- [ ] **Step 4: No commit (verification only)**

---

### Task 36: CHANGELOG + push branch + open PR

**Files:**
- Modify: `CHANGELOG.md` (project root)

- [ ] **Step 1: Add CHANGELOG entry**

Open `CHANGELOG.md` at the worktree root. Add a new section above `## [0.0.0]`:

```md
## [Unreleased]

### Changed
- web/: stripped multi-tenant (zones, auth, skills, cron, daemon manager,
  provider credentials, user/invite) — ~50 files deleted, ~25 edited
- shared/api/client.ts rewritten to match spec §4.1 (flat URLs, `X-Cocli-Token`
  auth header, `plugins`/`version`/`health`/`settings` exports added)
- web/ branding: `cocli` Inter wordmark, "c" SVG favicon, `cocli local` title

### Added
- First-run wizard (`web/src/components/wizard/`): 3-step flow with
  zustand `wizardStore`, localStorage persistence, `?skip-wizard=1` URL fallback
- Plugin manager mockup at `/settings/plugins`: full CRUD against zustand
  `pluginsStore` with token-reveal-once flow per spec §4.4
- `shared/api/mock.ts` stub + `VITE_USE_MOCK=true` short-circuit for
  backend-less dev runs

### Fixed
- ESLint to zero (16 residual `no-explicit-any` + `no-empty` errors in surviving
  test files)
```

- [ ] **Step 2: Commit CHANGELOG**

```bash
git add CHANGELOG.md
git commit -m "changelog: web multi-tenant strip + wizard + plugin manager + branding + lint"
```

- [ ] **Step 3: Verify full branch state**

```bash
git log --oneline main..HEAD | wc -l
echo "commits on this branch: ^^"
git diff --stat main..HEAD | tail -3
```

Expected:
- Commit count: ~25-30 (one per task that produced changes)
- Diff stat: net negative line count (delete-heavy)

- [ ] **Step 4: Push branch**

```bash
git push -u origin worktree-web-multitenant-strip-and-polish
```

- [ ] **Step 5: Open PR**

```bash
gh pr create --title "web: multi-tenant strip + first-run wizard + plugin manager mockup + branding + ESLint cleanup" --body "$(cat <<'EOF'
## Summary
- Strip multi-tenant artefacts from `web/` + `shared/` (delete + flatten + shim,
  not feature-flag hide). ~50 files deleted, ~25 edited, ~7 added.
- Stores-only first-run wizard (3 steps: locate Claude → create first agent →
  guide to `#general`), persisted via localStorage, `?skip-wizard=1` fallback.
- Full-CRUD plugin manager UI mockup at `/settings/plugins` against zustand
  `pluginsStore`, including token-reveal-once flow per spec §4.4.
- `shared/api/client.ts` rewritten as the spec §4.1 contract the M0.0.1 Rust
  backend will implement; `VITE_USE_MOCK=true` short-circuits to an 80-LOC
  stub for backend-less dev.
- Branding: `cocli` Inter wordmark, "c" SVG favicon, `cocli local` title.
- ESLint to zero.

Spec: `docs/superpowers/specs/2026-05-21-web-multitenant-strip-and-polish-design.md`
Plan: `docs/superpowers/plans/2026-05-21-web-multitenant-strip-and-polish.md`

## Test plan
- [ ] `npm install && npm run lint` (web/) exits 0
- [ ] `npm test` (web/) all pass
- [ ] `npm run build` (web/) succeeds
- [ ] `VITE_USE_MOCK=true npm run dev` smoke per spec §10 checklist:
  - wizard 3-step flow works end-to-end
  - `/settings/plugins` register → reveal → revoke flow works
  - refresh persists plugin list, `localStorage.clear()` re-triggers wizard
- [ ] `git grep` shows zero zone/SaaS references

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 6: Return the PR URL**

The output of `gh pr create` includes the PR URL. Save it for the followup memory entry.

---

## Self-review notes

This plan covers spec sections 1-10. Section 11 (open questions / deferred) and 12 (execution sketch) are reflected in the plan's milestone bucketing.

Known not-explicitly-tasked items handled implicitly:
- `workspacePanelStore.ts` panel-type union shrink — handled in Task 14 alongside the App.tsx zone_* branch removal.
- `web/src/api/client.ts` (3-LOC re-export shim) needs no change; it follows `shared/api/client.ts` automatically.
- `web/src/api/client.skill-library.test.ts` — should be deleted in Phase 1 if it transitively imports deleted skill exports. If it didn't get deleted via the zone-component sweep, delete it in Task 8 (when `zoneSkillLibrary` export goes).


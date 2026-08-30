# Public Alpha Closeout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the remaining public-alpha path: conversation-first Channel UX, A5 recovery evidence through schema 19, then A6 installable artifacts.

**Architecture:** Keep the existing local loop (SQLite + loopback Axum + `web/src/local/LocalApp.tsx`). Do not grow Git/Workspace product surfaces, Task-as-Channel, plugins, Skill 3D, or cloud UI. A5 stays CLI-first (DESIGN: restore is offline). A6 ships direct archives plus user-scoped installers for five targets; Homebrew/MSI/deb are out of this plan.

**Tech Stack:** Rust 1.80 workspace, SQLite migrations 0001–0019, Axum local API, React 19 + Vite local client, GitHub Actions.

## Global Constraints

- `DESIGN.md` wins over older docs (`docs/LANDING.md`, omni pages dated 2026-07-17, `CHANGELOG.md`).
- Channel is conversation + membership + shared context. Tasks remain Bridge/API coordination primitives.
- Workspace is an optional resource handle; no Git/worktree product UX.
- Restore remains an offline CLI operation in alpha; the UI may download backups only.
- Listener stays loopback-only. No multi-tenant, billing, or cloud enrollment.
- Public alpha version line stays `0.0.x`. Do not advertise SemVer stability.
- Do not execute Skill/MCP remote sources, Runtime reload, or session-effective proof (Phase 3D).
- Do not implement plugin packaging (Beta B1) or bring Wiki back.
- Copy is domain-neutral: never call a Channel a project; never require a repository to start.
- Verification before claiming done: run the task’s listed commands and paste/record the actual output.

## Locked product decisions

These close open questions so implementers do not re-litigate them.

1. **Task UI demotion (closes `DESIGN.md` open question):** Channel primary chrome is Conversation, Members, and Memory. Coordination (`LocalTasksWorkspace`) is progressive disclosure (details/overflow), not a peer tab. Task HTTP/Bridge APIs stay. `channels.goal` stays in schema/API for compatibility; local UI never collects it.
2. **A5 product bar for alpha:** CLI portable backup, preflight, restore, and an explicit CLI rebind are enough. No in-app restore wizard.
3. **Portable sanitization:** Keep current wipes (installation row, Bridge tokens, working-state, in-flight deliveries, live sessions, binding secrets). Desired-state Skill/MCP profiles and bindings survive. After restore, hash-bound apply of a pre-restore approval must fail closed (stale). If a test shows it can apply, sanitization must also drop non-terminal apply runs and approval rows.
4. **A6 first channels:** GitHub Release archives + SHA-256 + user-scoped shell/PowerShell installers for five targets. Signing/notarization run only when release secrets exist; missing secrets still produce unsigned checksummed drafts and fail the “signed alpha” gate rather than silently claiming signed.
5. **Out of this closeout:** Intel Mac / Linux aarch64 CI (may land with A6 packaging if OpenSSL is cheap; otherwise stay documented as planned), Homebrew/Scoop/WinGet/pkg/MSI/deb/rpm, cloud `web/src/App.tsx` deletion, plugin crates.

## File map

| Path | Role in this plan |
|---|---|
| `DESIGN.md` | Close Task-UI question; keep restore-is-CLI. |
| `ROADMAP.md` | Flip A5/A6 status only after evidence exists. |
| `docs/SUPPORT_MATRIX.md` | Honest support surface for installers and recovery. |
| `docs/LANDING.md` | Replace stale “Workspace provider depth” remaining-work paragraph. |
| `CHANGELOG.md` | Record closeout under `[Unreleased]` then the `0.0.1` tag. |
| `omx_wiki/index.md`, `omx_wiki/public-alpha-distribution.md` | Point at new evidence; do not restart A4. |
| `web/src/local/LocalApp.tsx` | Channel chrome, membership surface, Task disclosure. |
| `web/src/local/LocalApp.test.tsx` | Empty Channel, membership, no Task-first empty state. |
| `web/src/local/localization.ts` | Conversation-first copy (en + zh-CN). |
| `web/src/local/LocalTasksWorkspace.tsx` | Keep; stop promoting it as Channel definition. |
| `web/src/main.tsx` | Remains `LocalApp` only. Do not rewire to `App.tsx`. |
| `bin/cocli/src/main.rs` | Add `rebind` subcommand. |
| `bin/cocli/src/portable.rs` | Schema 12/19 fixture restore tests live beside existing bundle tests. |
| `crates/cocli-store/src/lib.rs` | `CURRENT_SCHEMA_VERSION = 19`; `sanitize_portable_state`; `bind_workspace`. |
| `crates/cocli-store/tests/governance_migrations.rs` | Pattern for 0012/legacy Skill fixtures. |
| `crates/cocli-api/src/lib.rs` | Existing `PUT /api/workspaces/:id/binding`; CLI wraps Store, not HTTP. |
| `scripts/install.sh` | Replace placeholder. |
| `scripts/install.ps1` | New Windows installer. |
| `scripts/smoke-installed.sh` | Clean-prefix fake-runtime smoke for A6 gates. |
| `.github/workflows/release.yml` | Replace noop placeholder. |
| `.github/workflows/ci.yml` | Keep primary 3-target matrix; optional extra targets only if packaging needs them. |

## DAG

```
H1 hygiene (optional rebase onto origin/main; separate from feature PRs)
        │
        ├──────────── Track C (conversation-first) ──── C1 → C2 → C3 → C4
        │
        └──────────── Track R (A5 evidence) ─────────── R1 → R2 → R3 → R4 → R5
                                                              │
                                                              ▼
                                                    Track D (A6) D1 → D2 → D3 → D4
```

C and R are independent. D requires R5 (migration fixture used as a release gate). Do not start D3/D4 until a schema-19 portable fixture exists.

---

## Track H — Hygiene (do first, tiny)

### Task H1: Isolate origin/main dependency drift

**Files:** none in the feature branches.

Local `main` is 7 commits behind `origin/main` (Dependabot: react-dom, vitest, thiserror 1→2, tempfile, vite, …). Feature work must not mix those bumps.

- [ ] **Step 1:** On a throwaway check: `git fetch origin && git log --oneline HEAD..origin/main`
- [ ] **Step 2:** Either rebase a clean feature branch onto `origin/main` before C/R/D, or land C/R/D on current HEAD and rebase later. Prefer rebase-first if `thiserror` 2.x does not explode the workspace.
- [ ] **Step 3:** If `thiserror` 2.x breaks compile, leave the bump out of the closeout PRs; do not “fix while passing.”

**Done when:** Feature commits do not contain unrelated dependency noise.

---

## Track C — Conversation-first subjects

### Task C1: Copy and empty states talk first

**Files:**
- Modify: `web/src/local/localization.ts`
- Modify: `web/src/local/LocalApp.tsx` (composer labels only if they still say “task”)
- Test: `web/src/local/LocalApp.test.tsx`

**Decision:** User-facing Channel composer copy uses “message” / “conversation”, not “task”. Keep Task strings that belong to `LocalTasksWorkspace`.

- [ ] **Step 1:** Write a failing test in `LocalApp.test.tsx` that opens a Channel with zero messages and zero tasks and asserts:
  - heading `Start the conversation` (or the current `startConversation` string)
  - no heading/button whose accessible name is `Tasks` as a required next step
  - create-channel form has Name (and optional description), **no** Goal field
- [ ] **Step 2:** Run `cd web && npx vitest run src/local/LocalApp.test.tsx`
  Expected: FAIL until chrome/copy match.
- [ ] **Step 3:** Remove or rewrite leftover Channel-level keys that still frame work as a Task (`startTask`, `startTaskDescription`, `taskFor`, `taskPlaceholder`, `runTask` if still referenced from Channel composer). Keep `onboardingTask` meaning “send the first message” or rename the key to `onboardingMessage` in both `english` and `chinese` objects.
- [ ] **Step 4:** Re-run the test file. Expected: PASS.
- [ ] **Step 5:** Commit `test(web): assert conversation-first channel empty state`

### Task C2: Demote Task chrome; elevate Members

**Files:**
- Modify: `web/src/local/LocalApp.tsx` (`WorkspaceView`, Channel `local-secondary-nav`, around the current Tasks button ~1017–1044 and the `workspaceView === 'tasks'` branch ~1852)
- Modify: `web/src/local/local.css` only if a Members pane needs layout (prefer existing tokens)
- Keep: `web/src/local/LocalTasksWorkspace.tsx`

**Produces:**
- `WorkspaceView` includes `'members'`
- Channel secondary nav order: Conversation, Members, Memory
- Tasks reachable from a disclosure control with `t('tasksWorkspaceHint')`, default closed
- Members pane lists `channelAgents`, invite, pause/resume (move existing sidebar membership controls into this pane rather than inventing a second membership model)

- [ ] **Step 1:** Extend the C1 test: Channel nav has buttons Conversation / Members / Memory; Tasks is **not** a sibling tab. Opening the disclosure still renders coordination UI (`tasksTitle` / `Coordination`).
- [ ] **Step 2:** Run vitest; expect FAIL (Tasks is still a peer tab).
- [ ] **Step 3:** Implement nav + members pane + disclosure. Default `workspaceView` for a selected Channel remains `'chat'`. Deep-link/query for tasks is unnecessary in alpha.
- [ ] **Step 4:** Keyboard: all three primary tabs and the Tasks disclosure are focusable. Do not communicate Agent state by color alone (existing status text stays).
- [ ] **Step 5:** `cd web && npx vitest run src/local/LocalApp.test.tsx && npm run lint`
  Expected: PASS.
- [ ] **Step 6:** Commit `fix(web): demote channel task board to progressive disclosure`

### Task C3: Membership and Agent-entry regressions

**Files:**
- Test: `web/src/local/LocalApp.test.tsx`
- Modify: `web/src/local/LocalApp.tsx` only if a regression appears

- [ ] **Step 1:** Add/keep tests for:
  - create Channel → invite Agent → send message (existing product-loop test must still pass)
  - direct Agent conversation still the default Agent view
  - deleting/leaving language must not imply Tasks define the Channel
- [ ] **Step 2:** `cd web && npm test && npm run lint && npm run build`
  Expected: PASS (match current suite size ± the new cases).
- [ ] **Step 3:** Commit `test(web): cover membership and agent conversation after task demotion`

### Task C4: Close the DESIGN question

**Files:**
- Modify: `DESIGN.md` open questions
- Modify: `ROADMAP.md` product-priority bullet 1 if it still reads as unfinished after C2

- [ ] **Step 1:** Mark the Task-UI question `[x]` with: hidden as a Channel peer tab; available as optional coordination disclosure; Bridge Task APIs unchanged.
- [ ] **Step 2:** Commit `docs: close channel task-ui demotion decision`

**Track C done when:** A new Channel empty state offers talk or invite, not a Task board or purpose field; Tasks still work for Agents.

---

## Track R — A5 recovery evidence

Current facts to preserve:
- `CURRENT_SCHEMA_VERSION = 19` in `crates/cocli-store/src/lib.rs`
- Portable format `cocli-portable-backup` v1 in `bin/cocli/src/portable.rs`
- Existing round-trip: `bundle_preflight_inventory_and_moved_git_rebind_round_trip`
- Sanitization in `sanitize_portable_state` (installation, tokens, working-state, sessions, secrets)
- Historical dogfood evidence stopped at schema **12** (`omx_wiki/session-log-portable-backup-rebinding.md`)
- Rebind HTTP exists: `PUT /api/workspaces/:workspace_id/binding` in `crates/cocli-api/src/lib.rs`; **no CLI**

### Task R1: Schema 12 → 19 restore fixture

**Files:**
- Create: `crates/cocli-store/tests/fixtures/schema12_pre_governance.sql` **or** a generated sqlite under `crates/cocli-store/tests/fixtures/` checked in as a small SQL dump (prefer SQL so it is reviewable)
- Modify: `crates/cocli-store/tests/governance_migrations.rs` or new `crates/cocli-store/tests/portable_schema_upgrade.rs`
- Pattern: existing `MCP_MIGRATIONS` / `LEGACY_SKILL_MIGRATIONS` constants in `governance_migrations.rs`

**Fixture must contain (schema 12 tables only):** 1 Channel, 1 Agent, 1 membership, 1 user message, 1 memory document, 1 Task, 1 Workspace descriptor + source binding. No 0013+ tables.

- [ ] **Step 1:** Write a test that opens the schema-12 fixture with a pool **without** running the normal Store migrator first, then `Store::open` (which applies 0013–0019), then asserts Channel/Agent/message/memory/task ids survive and `MAX(version) FROM cocli_schema_migrations = 19`.
- [ ] **Step 2:** `cargo test -p cocli-store portable_schema_upgrade -- --nocapture`
  Expected: FAIL until fixture + migrator path exist.
- [ ] **Step 3:** Add the fixture; reuse the name-reconciliation path for Skill-dev lineage only if you also add that variant (optional; 0012-clean is the required one).
- [ ] **Step 4:** Re-run. Expected: PASS.
- [ ] **Step 5:** Commit `test(store): restore schema 12 fixture through governance migrations`

### Task R2: Schema 19 portable bundle with Memory + governance desired state

**Files:**
- Modify: `bin/cocli/src/portable.rs` tests
- Possibly: `crates/cocli-store/src/lib.rs` only if inventory needs extra counts (default: **do not** change inventory schema; assert via SQL after restore)

**Source store (schema 19) must include:**
- Subjects from R1 plus
- one Skill profile + binding
- one MCP profile + binding
- one Memory document

- [ ] **Step 1:** Test `create_bundle` → `preflight_bundle` → `restore_bundle` into a fresh data dir.
- [ ] **Step 2:** Assert:
  - `manifest.schema_version == CURRENT_SCHEMA_VERSION` (19)
  - restored installation id ≠ source
  - `agent_bridge_tokens` count is 0
  - no `running` agents
  - Channel/Agent/Memory rows survive by id
  - Skill profile id and MCP profile id survive
  - current Workspace binding is `unbound`; source binding remains a hint
- [ ] **Step 3:** `cargo test -p cocli --bin cocli -- portable`
  Expected: PASS including the old Git rebind test.
- [ ] **Step 4:** Commit `test(cli): portable backup round-trip through schema 19 governance rows`

### Task R3: Stale apply must fail closed after restore

**Files:**
- Test: `crates/cocli-api/tests/governance_integration.rs` and/or store tests
- Modify: `crates/cocli-store/src/lib.rs` `sanitize_portable_state` **only if** a restored approval can still apply

- [ ] **Step 1:** Seed an approved Skill or MCP plan, portable-restore into a new installation, then attempt apply with the restored approval id / hashes.
- [ ] **Step 2:** Expected: apply rejected as stale/mismatch/not-found. If it succeeds, FAIL the task and delete non-terminal apply runs + approval rows during sanitization, then retest.
- [ ] **Step 3:** `cargo test -p cocli-api governance` (or the focused test binary you added)
  Expected: PASS.
- [ ] **Step 4:** Commit `test(store): portable restore cannot replay governance apply`

### Task R4: CLI rebind

**Files:**
- Modify: `bin/cocli/src/main.rs` (`Command` enum)
- Modify: `bin/cocli/src/portable.rs` or a small `rebind.rs`
- Test: `bin/cocli/src/portable.rs` (extend the existing moved-git test to call the CLI-equivalent function)

**Interface:**

```text
cocli --data-dir <dir> rebind --workspace-id <uuid> --local-locator <path>
```

Wraps `Store::open` + `Store::bind_workspace`. Does not start the HTTP server. Refuses if the server pidfile is live (reuse `cocli-pidfile` if already wired; otherwise document “run only while stopped” and check the SQLite lock).

- [ ] **Step 1:** Unit-test the rebind helper: unbound after restore → ready after rebind to a moved path (same assertions as the existing portable test, but through the new helper).
- [ ] **Step 2:** Implement the clap subcommand. Help text must say this binds a **resource handle**, not a Git product workflow.
- [ ] **Step 3:** `cargo test -p cocli --bin cocli -- portable`
- [ ] **Step 4:** Commit `feat(cli): add workspace rebind subcommand for portable restore`

### Task R5: A5 evidence + status flip

**Files:**
- Create: `omx_wiki/session-log-a5-schema19-recovery.md` (short evidence: commands, schema 12 and 19, sanitization, rebind)
- Modify: `omx_wiki/index.md` current handoff
- Modify: `ROADMAP.md` A5 row → complete
- Modify: `docs/SUPPORT_MATRIX.md` portable row to mention `cocli rebind`
- Modify: `docs/LANDING.md` remaining-work paragraph (delete Workspace-provider-depth; A5 done; A6 remaining)
- Modify: `README.md` Backup and restore section with `rebind`

- [ ] **Step 1:** Record actual commands and hashes from R1–R4 runs (no invented checksums).
- [ ] **Step 2:** Flip status only after those commands exist in the evidence page.
- [ ] **Step 3:** Commit `docs: mark A5 recovery complete with schema 19 evidence`

**Track R done when:** A schema-12 database and a schema-19 portable bundle restore on a second data dir; subjects and desired-state governance survive; apply cannot replay; rebind is a documented CLI.

---

## Track D — A6 installable public alpha

Depends on: R2 fixture available for a migration gate. Does **not** depend on Track C, but ship C with the same tag if possible.

Contract source: `omx_wiki/public-alpha-distribution.md`.

### Task D1: Version, changelog, support matrix for a tag

**Files:**
- Modify: `Cargo.toml` workspace `version` → `0.0.1` when tagging (keep `0.0.0` until the release PR)
- Modify: `CHANGELOG.md` — replace the stale M0-only Unreleased story with what actually shipped, plus A5/A6
- Modify: `docs/SUPPORT_MATRIX.md` — installers move from “Not yet” when D2+D3 land, not before

- [ ] **Step 1:** Draft Unreleased notes covering: Agent/Channel model, portable backup, skill/MCP local governance, conversation-first chrome, CLI rebind. Do not claim signed installers until D2 secrets exist.
- [ ] **Step 2:** Commit `docs: refresh changelog for public alpha closeout` (version bump can be the release commit)

### Task D2: Real installers (unsigned path first)

**Files:**
- Replace: `scripts/install.sh`
- Create: `scripts/install.ps1`
- Create: `scripts/smoke-installed.sh`

**Installer requirements (from the distribution contract):**
- select version (env `COCLI_VERSION` or latest GitHub release) and OS/arch
- download `cocli` + `cocli-bridge` from the same release
- verify SHA-256 before replacing
- user-scoped prefix default `$HOME/.local` (Unix) / `%LOCALAPPDATA%\cocli` (Windows)
- no admin by default
- atomic replace; keep previous binaries on failure
- do not touch the data directory
- print version and next action: `cocli` then open `http://127.0.0.1:8090`

Until GitHub Releases exist, `install.sh` must:
- support `COCLI_ARTIFACT_DIR` pointing at locally built binaries for dogfood
- still checksum
- still install both binaries

- [ ] **Step 1:** Test the installer against `COCLI_ARTIFACT_DIR` in a temp prefix; assert `cocli --version` and `cocli-bridge --version`.
- [ ] **Step 2:** `scripts/smoke-installed.sh` starts with `--fake-runtime --data-dir <tmp>`, curls `/api/doctor` or `/api/runtimes`, creates nothing destructive, then SIGTERM.
- [ ] **Step 3:** Commit `feat(release): user-scoped installer and installed-prefix smoke`

### Task D3: Release workflow for five targets

**Files:**
- Replace: `.github/workflows/release.yml`

**Pipeline (unsigned first):**
1. On tag `v0.0.*` or `workflow_dispatch`
2. Verify tag matches workspace version, lockfile present
3. `web && npm ci && npm run build`
4. `cargo build --release --bin cocli --bin cocli-bridge` per target:
   - `x86_64-unknown-linux-gnu`
   - `aarch64-unknown-linux-gnu` (use `cross` or skip with an explicit job `if:` until OpenSSL is solved; do not silently omit from the contract table)
   - `x86_64-apple-darwin`
   - `aarch64-apple-darwin`
   - `x86_64-pc-windows-msvc`
5. Stage archives containing both binaries, licenses, short INSTALL.txt
6. `SHA256SUMS`
7. Optional signing jobs: `if: secrets.APPLE_… / WINDOWS_…` — missing secrets print `unsigned draft`
8. Upload a **draft** GitHub Release
9. Run `scripts/smoke-installed.sh` on linux/mac runners from the just-built archive
10. Restore the R2 (or R1) fixture with the release binary: `cocli restore --input …`

- [ ] **Step 1:** Keep PR CI unable to see release secrets (already true if secrets are environment-scoped).
- [ ] **Step 2:** Do not delete the primary 3-target `ci.yml` matrix.
- [ ] **Step 3:** Commit `ci: replace release placeholder with multi-target draft artifacts`

### Task D4: Mark A6 / first-use gates

**Files:**
- Modify: `ROADMAP.md` A6 status — `in progress` until a draft release exists on all **required** targets that actually built; `complete` only after clean-prefix smoke + restore gate on linux + macOS + windows
- Modify: `omx_wiki/public-alpha-distribution.md` with the actual archive names
- Modify: `docs/SUPPORT_MATRIX.md` prebuilt column
- Modify: `README.md` top status paragraph

First-use (already in `LocalApp`): after installed binary start, UI still opens Channel/Agent first. Do not add a Runtime Doctor wizard. `/api/doctor` may be linked from Settings only.

- [ ] **Step 1:** Run once from an installed prefix (not `cargo run`) and note the URL, onboarding card, and that no Workspace is required.
- [ ] **Step 2:** Flip ROADMAP only with that evidence.
- [ ] **Step 3:** Commit `docs: record public-alpha distribution status`

**Track D minimum ship:** unsigned checksummed archives + installers + smoke + restore fixture. Signed macOS/Windows is the same workflow with secrets, not a new product track.

---

## Explicitly not in this plan

- Skill governance Phase 3D (registry, credentials, session-effective)
- MCP Grok transactional writer
- Plugin SDK (`crates/cocli-plugin*`)
- Wiki-as-plugin
- Deleting cherry-picked `web/src/App.tsx` cloud shell
- Expanding Git provider UX
- Hard token/budget enforcement
- Intel Mac / Linux aarch64 **CI** if OpenSSL packaging is still blocked (document, don’t fake green)

## Suggested PR slices

1. `web: conversation-first channel chrome` (C1–C4)
2. `store/cli: schema 12–19 portable evidence + rebind` (R1–R5)
3. `release: installers + draft workflow` (D1–D4)

Do not combine 3 with 1.

## Definition of done (public alpha tag)

- [ ] Track C merged
- [ ] Track R merged; ROADMAP A5 = complete
- [ ] `scripts/install.sh` no longer exits 1 with a placeholder
- [ ] Draft GitHub Release contains `cocli` + `cocli-bridge` for the targets that built
- [ ] SHA-256 file attached
- [ ] Fake-runtime installed-prefix smoke passed on at least Linux and macOS
- [ ] Schema 12 or 19 fixture restored by the release binary
- [ ] SUPPORT_MATRIX matches reality (signed vs unsigned)
- [ ] Version `0.0.1` tagged; CHANGELOG has a dated section

# Skill + MCP Governance Integration

This integration preserves the completed Skill and MCP governance histories in
one buildable, migratable, runnable, and auditable branch. Both remain
supporting capability tracks beneath cocli's persistent Agent and Channel
model; this work adds no new governance feature.

## History and merge strategy

- Merge base: `cd3501ac4e0f0072384edd1e19e5794a053986b7`.
- MCP parent: `2dcbf6af3fb876bf253ba272eeb02d8ca47b8076`
  (`feat(mcp): add portable bundles and adapter conformance`).
- Skill parent: `2b27333416c6371ad33c34e219df0d84f5757eb2`
  (`feat(skills): add managed scopes and workspace lockfiles`).
- Integration branch: `codex/governance-integration`.
- Strategy: `git merge --no-ff --no-commit
  codex/skill-governance-phase-3c`; no squash, rebase, amend, or source-history
  rewrite.

The content conflicts and their semantic resolutions were:

| File | Resolution |
|---|---|
| `Cargo.toml` | Keep MCP `sha2` and Skill `unicode-normalization` workspace dependencies. |
| `crates/cocli-server/Cargo.toml` | Keep both direct dependencies used by the merged Runtime service. |
| `crates/cocli-api/src/lib.rs` | Register MCP plus all Skill governance routers; retain MCP apply locks, Skill mutation locks, Skill snapshot coordination, and the shared bridge lock in one `AppState`. |
| `crates/cocli-api/tests/local_loop.rs` | Union the MCP and Skill fake Runtime contracts, fixtures, and regression suites instead of dropping either side. |
| `crates/cocli-store/src/lib.rs` | Keep both Store modules and models; reserve MCP 0013-0016, move Skill to 0017-0019, and add exact-name development-lineage reconciliation. |
| `shared/api/client.ts` | Keep MCP inventory/doctor and all Skill governance clients; retain forced Skill snapshot refresh without duplicating methods. |
| `web/src/local/api.ts` | Keep all MCP operations and the force-refresh Skill inspection signatures. |
| `web/src/local/LocalApp.test.tsx` | Keep both MCP portability/conformance mocks and Skill scope/managed-store/lockfile/GC mocks. |

Auto-merged shared types, localization, CSS, Runtime/Skill server code, README,
and roadmap content were reviewed as a semantic union. Neither branch's whole
file replaced the other.

## SQLite migration contract

`cocli_schema_migrations.version` is the migration identity and primary key.
The runner applies its statically ordered list, records `version`, `name`, and
`applied_at`, and wraps each migration plus marker insert in one transaction.
It does not store or compare a SQL checksum. For governance versions 0013 and
later, the integrated runner additionally checks that an already-recorded
version has the expected name so a collision cannot be silently accepted;
published 0001-0012 retain their prior version-only replay semantics.

The final mapping is:

| Version | Name | History |
|---:|---|---|
| 0013 | `mcp_governance_phase_2a` | MCP profiles, bindings, plans, approvals |
| 0014 | `mcp_governance_phase_2b` | MCP apply runs |
| 0015 | `mcp_governance_phase_2c` | MCP capability and journal state |
| 0016 | `mcp_governance_phase_3a` | MCP bundle import audit |
| 0017 | `skill_governance` | Skill profiles, bindings, locks, plans, decision audit |
| 0018 | `skill_governance_apply_state` | Skill leases, runs, actions, recovery audit |
| 0019 | `skill_governance_managed_scopes` | Managed artifacts, materializations, adoption, workspace lockfiles, GC references |

Published migrations 0001-0012 are unchanged. Fresh and 0012 databases apply
0013-0019 in order. MCP-only databases append 0017-0019. Before normal
migration dispatch, an isolated Skill-development database is reconciled in a
single transaction by moving only exact `(version, name)` pairs
`(13, skill_governance)`, `(14, skill_governance_apply_state)`, and
`(15, skill_governance_managed_scopes)` to 17-19. MCP 13-16 then apply normally.
Existing Skill rows, artifacts, lockfiles, journals, approvals, and audit rows
are untouched. An unexpected name at a claimed version fails closed.

Each migration and its history marker commit together. A statement failure
therefore leaves no marker or partial schema and a later restart can retry after
the cause is removed. Reopening an already migrated database is idempotent.

## API, Store, Runtime, and desktop result

The API router exposes both `/api/runtimes/mcp/*` and
`/api/skills/governance/*` without duplicate registration or route shadowing.
The merged `AppState` keeps independent MCP apply-lock and Skill
mutation/snapshot lifecycles. Store queries and recovery scans use distinct MCP
and Skill tables, so one domain cannot consume the other's run identifiers or
non-terminal state. Runtime inventory/doctor aggregation preserves partial
failure: an unavailable adapter produces diagnostics without failing the other
inventory.

The desktop retains MCP Inventory, Profiles, Plan, Apply, Recovery,
Portability, and Conformance plus Skill Inventory, Profiles, Lock, Plan, Apply,
Recovery, Scopes, Managed Store, Materializations, Adoption, Workspace
Lockfile, and GC. The shared clients/types/localization/styles contain both
contracts. Workspaces load their own data only when selected, Skill native
probes remain coalesced and short-TTL cached, and force refresh stays explicit.
Responsive grids, `min-width: 0`, bounded code blocks, ellipsis/anywhere wraps,
empty states, loading messages, and independent error regions cover narrow
screens and long paths/fingerprints without redesigning the UI.

## Cross-governance safety

- MCP and Skill identifiers live in domain-specific tables and API paths.
  Approval records, apply runs, idempotency constraints, nonce checks, locks,
  journals, and recovery queries cannot collide merely because display names,
  hash prefixes, or caller-provided nonce text match.
- MCP source locks and Skill scoped leases are acquired inside separate apply
  flows; neither flow holds the other's lock, so there is no cross-domain lock
  ordering cycle.
- MCP secret references remain opaque and Skill artifact content is never
  treated as a credential. Both redaction/canary suites run against fake or
  temporary roots; plaintext canaries are rejected and not returned through
  API responses, diagnostics, SQLite fields, logs, or UI snapshots.
- Tests use temporary databases, homes, workspaces, Runtime configuration
  roots, managed stores, backups, and quarantine directories. They never use a
  real Runtime MCP destination or real user Skill root.
- Filesystem discovery and Runtime discovery remain configured/installed
  evidence only. Neither is labeled Session-effective.

## Validation matrix

| Area | Evidence |
|---|---|
| Migration lineages | Fresh, 0012-only, MCP-only, Skill-only development, repeated open, and failed-transaction recovery tests |
| Durable state | MCP bundle audit plus Skill artifact/lockfile/materialization and both apply/recovery journals survive reopen |
| Unified API | One file-backed temp-home integration creates, approves, applies, restarts, queries, rolls back, recovers, exports/imports, and verifies both domains |
| Rust | `cargo test --workspace --locked`; Clippy with all targets/features and warnings denied; stable rustfmt check; locked workspace build |
| Desktop | Vitest suite; ESLint for `web` and `shared`; TypeScript plus Vite production build |
| Safety | Before/after SHA-256 window for real Codex, Cursor, Claude, and Grok MCP configuration and Skill roots; secret-canary tests; staged diff review |
| Git | Unmerged-entry check, conflict-marker scan, `git diff --check`, cached diff check, and final `git show --check` |

## Explicitly unsupported

This integration does not add a remote Registry or Marketplace, dynamic
plugins, remote installation, installation-script execution, private
credential resolution, arbitrary target paths, Gateway behavior, automatic
Runtime restart, or automatic active-Session visibility claims. Such work
requires separate source, credential, Runtime, and authorization contracts.

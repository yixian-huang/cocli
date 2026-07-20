# Roadmap

The roadmap follows the product contract in [DESIGN.md](DESIGN.md): cocli is a
general-purpose local environment organized around persistent Agents and
Channels. Workspace providers (including Git) are thin adapters only—not a
product line. Channels are collaboration rooms, not project/purpose shells
strongly bound to Tasks.

## Current foundation

Implemented foundations include the local Rust server, SQLite state, embedded
React client, eight Runtime adapters, durable message delivery, optional Task
coordination primitives, Agent Memory and Skills, runtime history, live
execution events, global search, and recoverable backup/restore (including
portable bundles).

The durable subject migration is landed. Remaining alpha work focuses on
conversation-first Channel UX, state recovery and migration evidence,
distribution, onboarding, and cross-platform release—not Git Workspace product
depth or Task-centric Channel redesign.

## Alpha milestones

| Milestone | Outcome | Status |
|---|---|---|
| A1 — Durable subjects | Independent persistent Agents, Channels, many-to-many memberships, direct Agent conversation, lifecycle/execution state separation | complete |
| A2 — Agent self-organization | Capability-scoped Bridge operations for Agents to create Agents, Channels, memberships, and optional durable coordination work | complete |
| A3 — Subject-first client | Channels and Agents are primary navigation; conversation and membership define Channel; Memory, Skills, and diagnostics live under subjects | complete |
| A4 — Workspace providers | **Descoped as product work (2026-07-20).** Optional resource handles and thin adapters (including Git) may exist for Runtime cwd and portable rebind; cocli does **not** maintain Git/worktree/provider product features or provider-depth milestones. Existing schema/APIs stay for compatibility. | descoped |
| A5 — Recovery and portability | Restore/import, schema compatibility checks, migration safety, and documented cross-machine rebinding of **subjects and durable state** (not Git product workflows) | in progress |
| A6 — Installable public alpha | Signed/checksummed binaries, installer, release workflow, green cross-platform CI, accurate onboarding and support matrix | planned |

### Product priority after A3 (alpha)

1. **Conversation-first subjects** — Channel empty/primary surfaces emphasize talk and membership; demote Task boards and purpose/goal framing; keep Task APIs for Agent coordination without redefining Channel.
2. **A5 — state portability** — backup, preflight, restore, installation rebind for Agents/Channels/Memory and related durable rows.
3. **A6 — installable alpha** — release artifacts, installers, first-run that starts from Agent/Channel without path or Workspace setup.
4. Supporting Skill/MCP governance only where it serves multi-Runtime desktop use; no pivot into package-manager or Git product directions.

### Supporting capability track — Skill governance

- **Phase 1 — inventory and doctor (complete):** reuse the Skill Library,
  `agent_skill_installs`, Runtime drivers, local API, and desktop Skills
  workspace to show filesystem-discovered candidates, ordered search paths,
  Runtime compatibility, managed/external/broken state, scope and provenance,
  invalid frontmatter, broken symlinks, duplicate targets, and shadowing.
  Filesystem evidence is explicitly not treated as proof of Session visibility
  or activation, and no user-global Skill directory is mutated by discovery.
- **Phase 2A — native discovery evidence (complete):** the driver contract now
  supports read-only native Skill probes. Codex app-server `skills/list` and
  `grok inspect --json` are merged with filesystem inventory, including native
  source evidence, Runtime-reported disabled state, filesystem fallback, and
  probe-failure diagnostics. Native discovery still does not claim active
  Session visibility or activation.
- **Phase 2B — snapshot and diagnostic hardening (complete):** add explicit
  observation timestamps, bounded short-TTL caching, in-flight native-probe
  coalescing, force refresh, lightweight filesystem-only Agent lists, true
  machine Runtime inventory without synthetic Agents, partial-failure
  diagnostics, and stable Skill/issue fingerprints with grouped root causes.
- **Phase 3A — read-only governed desired state (complete):** add versioned
  SkillProfile documents, machine/workspace/Agent profile bindings, deterministic
  effective desired-state inheritance, same-layer conflict reporting, immutable
  lock snapshots, stable SHA-256 observation/config/lock/plan hashes, drift and
  dry-run plan previews, approval/rejection audit rows, optimistic
  `expectedVersion` checks, approval staleness checks, and the
  `/api/skills/governance` API surface. Runtime and filesystem Skill evidence
  remains read-only, Cursor native Skill/session probing is explicitly
  unsupported by current stable CLI contracts, and no discovery result is
  treated as Session-effective proof.
- **Phase 3B — governed apply and verification (complete for the safe local
  subset):** apply only approved, non-stale plans whose observation, desired,
  and lock hashes still match. The first automatic writer established
  Runtime-target-derived Skill entries for digest-verified local or
  cocli-vendored copy/symlink actions,
  cocli-managed or symlink removal through quarantine, scoped leases, backup
  manifests, staging plus atomic rename, force-refresh verification, CAS-safe
  rollback, idempotent retries, and recovery-required state. Remote downloads,
  private credentials, Git clone, Registry/Marketplace sources, installation
  scripts, Runtime reload, and Session-effective adapters remain blocked/manual
  until stable source and Runtime contracts exist. Phase 3C extends this writer
  to canonical machine, Workspace, and Agent scopes and real Workspace
  lockfiles.
- **Phase 3C — canonical scopes, materialization, lockfile, and GC contracts
  (complete for the safe local governance loop):** define machine/user,
  workspace/project, and Agent scope semantics; expose Runtime capability
  evidence for runtime-specific and shared Skill roots; block reserved roots,
  legacy command roots, whole-root symlink takeover, symlink escape, read-only
  roots, cross-filesystem atomic-rename hazards, and out-of-scope roots; persist
  immutable managed artifacts, per-Skill materializations, ownership state (`managed`, `adopted`,
  `unmanaged`, `foreign`), adoption audit, workspace lockfile records with CAS
  and restore metadata, and GC protection references. Versioned HTTP APIs and
  the Skills workspace expose Scopes, Managed Store, Materializations,
  three-mode Adoption, Workspace Lockfile, and GC. Approved apply supports
  capability-approved machine, Workspace, and Agent targets, uses the managed
  store for per-Skill copy/symlink materialization, and performs real Workspace
  lockfile writes with journaled backup and rollback. GC is preview/nonce/CAS
  protected and quarantines managed artifact bytes before deletion. This phase
  does not add remote source support, install-script execution, arbitrary target
  paths, Runtime reload, or Session-effective proof.
- **Phase 3D — remote sources and Runtime/session integration (planned):** add
  explicit Registry/Marketplace and private-source credential policy without
  executing untrusted installation scripts; add Runtime reload adapters and
  session-bound verification only where a Runtime publishes a stable native
  contract. Filesystem or discovery evidence continues to require a new Session
  and must not be promoted to Session-effective proof.

This track remains subordinate to cocli's persistent Agent and Channel model;
it is Runtime governance for multi-Agent desktop work, not a standalone Skill
package-manager direction.

### Supporting capability track — MCP governance

- **Phase 1 — inventory and doctor (complete):** discover redacted definitions,
  probe Runtime-native state, preserve independent evidence fields, and report
  drift, duplicates, approval/authentication gaps, startup failures, and probe
  failures without modifying Runtime configuration.
- **Phase 2A — profiles and deterministic planning (complete):** persist
  versioned Runtime-neutral profiles and canonical machine/Workspace/Agent
  bindings; resolve `machine < workspace < agent` desired state with explicit
  same-level conflicts; generate stable, evidence-bound, fully redacted
  dry-run plans; and record hash-bound approve/reject decisions that become
  stale after desired-state or observation drift. Approval authorizes a future
  operation only and is never treated as applied.
- **Phase 2B — apply, reload, verify, and recovery (complete):** consume only
  live, hash-matching approvals; isolate Runtime adapter writers behind CAS,
  per-source locks, pre-write backups, and atomic subtree updates; preserve
  action-level partial-failure evidence; defer active-session reloads; verify
  against a fresh inventory; and expose audited rollback.
- **Phase 2C — native adapter negotiation and durable recovery (complete):**
  add a versioned capability matrix for Codex, Cursor, Claude, and Grok; bind
  plan hashes to adapter capabilities and binary/config schema evidence; expose
  preflight previews; persist action-level apply journals with idempotency
  keys, backup references, recovery-required states, rollback evidence, and
  saga-style partial-failure results; and verify with fresh inventory/doctor
  readback while reporting new-session-only activation separately from active
  Session effectiveness.
- **Phase 3A — portable bundles and adapter conformance (complete):** export
  deterministic governance bundles containing profiles, relative bindings,
  desired servers, opaque secret references, provenance, optional capability
  expectations, portability diagnostics, and a stable content hash; require
  explicit import rebinding for machine/Workspace/Agent/Runtime/secret and
  machine-local values; commit imports only to desired-state profiles and
  bindings without approval/apply side effects; and expose a library-only
  adapter SDK plus conformance harness for redaction, capability evidence,
  unsupported downgrade, write confinement, reload/verify, and recovery.

MCP governance does not introduce a Gateway or Registry and is not a secret
store. Unsupported/authentication actions remain blocked, Grok write support
stays manual until a stable transactional writer exists, and cocli never
restarts an active Runtime session without separate future authorization.

### Supporting governance integration (complete)

The completed Skill and MCP histories now run together through one Store, API,
and desktop without promoting either track into the Agent/Channel core model.
The integrated migration sequence reserves 0013-0016 for MCP and 0017-0019 for
Skill governance, reconciles the temporary Skill-only 0013-0015 development
lineage by exact recorded name, and regression-tests fresh, 0012, MCP-only,
Skill-only, restart, and failed-migration recovery paths. Domain-specific
approval, nonce, idempotency, lock, journal, run, audit, bundle, artifact, and
lockfile state remains isolated.

## Beta milestones

| Milestone | Outcome |
|---|---|
| B1 — Stable extension contract | Capability, permission, lifecycle, and storage contracts for optional plugins |
| B2 — Optional knowledge plugins | Wiki or other knowledge products implemented outside the core subject model |
| B3 — Community Runtime adapters | Documented third-party adapter SDK and compatibility suite |
| B4 — Stable local API | Versioned APIs, migration guarantees, deprecation policy, and external integration examples |

## Core completion criteria

The core product is complete for v1 when all of these are continuously tested:

1. Useful work can begin from either a Channel or Agent without a Project,
   repository, directory, Workspace, Task, or purpose field.
2. An Agent can participate in multiple Channels, and deleting a Channel does
   not delete the Agent.
3. Direct Agent conversation preserves one durable message substrate while
   hiding its system-managed private Channel.
4. Authorized Agents can create durable Agents and Channels through audited,
   idempotent Bridge operations; optional Tasks remain available for
   coordination without defining the Channel.
5. Agent identity, instructions, Memory, Skills, memberships, and conversation
   survive Runtime restarts and model changes.
6. Normal users can operate through working/waiting/paused/error states without
   understanding Session, Turn, PID, or CLI concepts; diagnostics remain
   available when needed.
7. Workspace is optional infrastructure only; Git and worktree behavior is not
   a product surface or milestone track.
8. Durable subject state is searchable, backed up, restored, migrated, and
   verified.

## Explicit non-goals

- Multi-tenant authentication, hosted billing, and cloud operations
- Central intelligent task assignment; Agents use durable claim/dependency
  primitives to organize work when needed
- Channel-as-project: purpose/goal objects or Task boards as the definition of
  a Channel
- Git Workspace / worktree / provider product features, onboarding, or depth
  milestones (thin adapters for path resolution and rebind only)
- Agent-owned diff review, checkpoint policy, rollback policy, or validation
  judgment inside cocli
- Hard cross-Runtime token or budget enforcement
- Requiring software-development concepts in the base Agent contract
- Wiki as a core product module; it is reserved for a future plugin

## Stability

`0.0.x` is alpha and may include forward migrations with documented transition
paths. `0.1.x` is beta: stored user data remains migratable and public APIs gain
deprecation periods. `1.x` follows SemVer and stable migration guarantees.

The reusable Runtime/Driver crates maintain their independent `0.1.x` line and
compatibility policy in [docs/runtime-ownership.md](docs/runtime-ownership.md).

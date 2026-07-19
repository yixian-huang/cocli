# Roadmap

The roadmap follows the product contract in [DESIGN.md](DESIGN.md): cocli is a
general-purpose local environment organized around persistent Agents and
Channels. Project and Git workflows are optional Workspace adapters.

## Current foundation

Implemented foundations include the local Rust server, SQLite state, embedded
React client, eight Runtime adapters, durable message delivery, Channel Tasks
and dependencies, Agent Memory and Skills, runtime history, live execution
events, global search, and recoverable SQLite backup/restore.

The durable subject migration is now landed. Remaining alpha work focuses on
Workspace provider depth, portable rebinding, distribution, onboarding, and
cross-platform release evidence.

## Alpha milestones

| Milestone | Outcome | Status |
|---|---|---|
| A1 — Durable subjects | Independent persistent Agents, Channels, many-to-many memberships, direct Agent conversation, lifecycle/execution state separation | complete |
| A2 — Agent self-organization | Capability-scoped Bridge operations for Agents to create Agents, Channels, memberships, and durable work | complete |
| A3 — Subject-first client | Channels and Agents become primary navigation; Tasks and shared context live under Channels; Memory, Skills, Workspace, and diagnostics live under subjects | complete |
| A4 — Workspace providers | Optional managed, directory, Git, and external Workspace attachments without making any provider a startup prerequisite | in progress |
| A5 — Recovery and portability | Restore/import, schema compatibility checks, migration safety, and documented cross-machine rebinding | in progress |
| A6 — Installable public alpha | Signed/checksummed binaries, installer, release workflow, green cross-platform CI, accurate onboarding and support matrix | planned |

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
- **Phase 2B — remaining probes and governed changes (planned):** add a stable
  Cursor native probe, explicit plan/apply/verify operations, and lockfile plus
  drift governance.

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
- **Phase 2B — apply, reload, verify, and recovery (planned):** introduce
  Runtime adapter writers, pre-write backups, rollback, explicit apply and
  reload controls, and post-apply verification against fresh native evidence.

MCP governance does not introduce a Gateway or Registry and is not a secret
store. Phase 2A contains no Runtime configuration writer or apply endpoint.

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
   repository, directory, or Workspace.
2. An Agent can participate in multiple Channels, and deleting a Channel does
   not delete the Agent.
3. Direct Agent conversation preserves one durable message substrate while
   hiding its system-managed private Channel.
4. Authorized Agents can create durable Agents and Channels and organize Tasks
   through audited, idempotent Bridge operations.
5. Agent identity, instructions, Memory, Skills, memberships, and work state
   survive Runtime restarts and model changes.
6. Normal users can operate through working/waiting/paused/error states without
   understanding Session, Turn, PID, or CLI concepts; diagnostics remain
   available when needed.
7. Workspace is optional and domain-neutral; Git and worktree behavior is an
   adapter rather than a global product assumption.
8. Durable state is searchable, backed up, restored, migrated, and verified.

## Explicit non-goals

- Multi-tenant authentication, hosted billing, and cloud operations
- Central intelligent task assignment; Agents use durable claim/dependency
  primitives to organize work
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

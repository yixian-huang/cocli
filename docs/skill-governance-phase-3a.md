# Skill governance Phase 3A

Phase 3A adds read-only governed desired state for desktop Skills. It records
what should exist, compares that desired state with current Runtime evidence,
creates deterministic lock previews, and stores dry-run plans for approval. It
does not apply changes to Runtime or filesystem Skill directories.

## Contract boundary

Phase 3A separates four evidence levels:

| Evidence | Meaning |
|---|---|
| `machine_discovered` | A Skill candidate was observed in a machine/user Runtime filesystem search root. |
| `runtime_discovered` | A Runtime native probe returned the Skill for the probed working directory. |
| `agent_workspace` | A Skill candidate exists in an Agent workspace path. |
| `session_effective` | A concrete active Runtime Session proved it loaded or activated the Skill. |

Current adapters do not produce `session_effective` evidence. `sessionEffective`
therefore remains `unknown` unless a future session-bound native contract
provides direct proof.

Filesystem discovery never implies that a running Session loaded a Skill.
Codex app-server `skills/list`, Grok `inspect --json`, Cursor filesystem
inventory, and Cursor capability checks also do not imply Session effectiveness.

## Cursor contract

Current official Cursor Agent CLIs expose no stable read-only command or
protocol for listing Agent Skills, and no stable contract that binds a Skill to
a concrete running Session. Phase 3A therefore:

- performs a bounded Cursor CLI capability check;
- returns structured unsupported/manual governance evidence when no native
  Skill/session contract is available;
- falls back to filesystem inventory for Cursor Skill candidates;
- does not start a Cursor Agent Session to infer Skill state;
- does not claim Cursor Session activation.

## Desired state

Desired state is stored in versioned `SkillProfile` documents. A profile uses
`schemaVersion: 1` and contains desired Skill entries with these fields:

| Field | Purpose |
|---|---|
| `logicalIdentity` | Runtime-neutral Skill identity used for matching and hashing. |
| `source` | `git`, `http`, `https`, or `local` source descriptor. Inline URL credentials are rejected; use `credentialRef`. |
| `version` / `resolvedRevision` | Optional resolved version metadata for pinned or tracked sources. |
| `contentDigest` / `manifestDigest` | Expected content and manifest hashes. |
| `targetRuntime` | Runtime the Skill is intended for. |
| `installScope` | `machine`, `workspace`, or `agent`. |
| `installationMode` | `copy`, `symlink`, `native`, or `manual`. |
| `enabled` | Desired enabled state. |
| `updatePolicy` | `pinned`, `manual`, or `track_revision`. |
| `allowedSources` | Optional source-kind allowlist. |
| `riskPolicy` | `trusted`, `allowlisted`, `approval_required`, or `blocked`. |
| `expectedDestination` | Optional expected destination path or native destination key. |

Profiles bind at machine, workspace, or Agent scope. Effective desired state is
computed in this order:

1. machine profile bindings;
2. workspace profile bindings;
3. Agent profile bindings.

Later scopes override earlier scopes for the same normalized
`logicalIdentity + targetRuntime`. Multiple different desired values in the
same scope produce a same-layer conflict; cocli reports the conflict and does
not select a winning value for that identity.

## Storage

Phase 3A persists governance state in SQLite:

| Table | Contents |
|---|---|
| `skill_profiles` | Opaque profile JSON plus `version`, `created_at`, and `updated_at`. |
| `skill_profile_bindings` | Machine/workspace/Agent profile bindings with `version`. |
| `skill_lock_snapshots` | Immutable lock preview snapshots with observation, desired, and lock hashes. |
| `skill_governance_plans` | Dry-run plan JSON, hashes, status, and `version`. |
| `skill_governance_plan_audit` | Approval, rejection, and stale-transition audit rows. |

Profile updates/deletes, binding deletes, and plan decisions require the current
`expectedVersion`. A stale version returns a conflict instead of overwriting the
current row.

## Hashes and ordering

Governance hashes use stable SHA-256 values over canonical JSON:

| Hash | Inputs |
|---|---|
| `snapshotHash` | Sorted observed Skills and diagnostics, excluding observation timestamps. |
| `desiredConfigHash` | Effective desired Skills plus same-layer conflicts. |
| `lockfileHash` | Deterministically ordered lockfile preview content. |
| `planHash` | Deterministically ordered dry-run actions plus observation/config/lock hashes. |

Observed Skills, diagnostics, lock entries, drift rows, and plan actions are
sorted before hashing or serialization. This keeps equivalent governance input
stable across repeated previews.

Workspace-scoped previews are candidates for a future reviewable workspace
lockfile. Machine- and Agent-scoped desired state remains cocli-owned SQLite
state and is never represented as an implicit workspace file. The API reports
this boundary as `workspace_candidate` or `store_only`; Phase 3A writes neither
form to a filesystem.

## Drift and plans

Drift comparison classifies read-only differences between the observed state and
the lock preview:

- `missing`
- `extra`
- `version_mismatch`
- `content_mismatch`
- `manifest_mismatch`
- `source_mismatch`
- `mode_mismatch`
- `shadowed`
- `broken_symlink`
- `unknown_evidence`
- `unsupported`
- `enabled_mismatch`

Dry-run plans map drift into these action types:

- `install`
- `update`
- `enable`
- `disable`
- `remove`
- `relink_copy`
- `lockfile_update`
- `manual`
- `unsupported`

Every action carries `expectedObservationHash`, `expectedConfigHash`, and
`expectedLockHash`. Approving a plan refreshes evidence and recomputes desired
state and lock content. If any of those hashes changed, the plan becomes stale
and approval fails with stale reasons:

- `observation_hash_changed`
- `desired_config_hash_changed`
- `lockfile_hash_changed`

Approving a Phase 3A plan records approval only. It returns `applied: false` and
`dryRun: true`.

## API

The Phase 3A API is under `/api/skills/governance`.

| Endpoint | Method | Purpose |
|---|---|---|
| `/profiles` | `GET` | List Skill profiles. |
| `/profiles` | `POST` | Create a profile. |
| `/profiles/:profile_id` | `GET` | Read one profile. |
| `/profiles/:profile_id` | `PUT` | Update a profile with `expectedVersion`. |
| `/profiles/:profile_id?expectedVersion=N` | `DELETE` | Delete a profile with optimistic concurrency. |
| `/bindings` | `GET` | List profile bindings, optionally filtered by `scope` and `scopeId`. |
| `/bindings` | `POST` | Bind a profile to machine, workspace, or Agent scope. |
| `/bindings/:binding_id?expectedVersion=N` | `DELETE` | Delete a binding with optimistic concurrency. |
| `/desired/effective` | `GET` | Return effective desired state for optional `workspaceId` and `agentId`. |
| `/evidence?force=true` | `GET` | Return governance observation from current inventory evidence. |
| `/lock/preview` | `POST` | Create an immutable lock snapshot and return a serialized lock preview. |
| `/locks` | `GET` | List lock snapshots for `scope` and `scopeId`. |
| `/plans` | `GET` | List dry-run governance plans for `scope` and `scopeId`. |
| `/plans` | `POST` | Generate and store a dry-run plan. |
| `/plans/:plan_id` | `GET` | Read one plan. |
| `/plans/:plan_id/audit` | `GET` | Read approval, rejection, and stale-transition audit rows. |
| `/plans/:plan_id/approve` | `POST` | Approve a draft plan with `expectedVersion`; does not apply changes. |
| `/plans/:plan_id/reject` | `POST` | Reject a plan with `expectedVersion`. |

`scope=machine` always normalizes `scopeId` to `machine`.

## Safety limits

Phase 3A is intentionally read-only outside SQLite governance tables:

- no writes to Runtime Skill directories;
- no writes to user-global Skill directories;
- no writes to workspace Skill directories;
- no lockfile writes to workspaces;
- no script execution;
- no downloads;
- no Runtime reload;
- no apply operation.

`/api/skills/governance/lock/preview` reports
`writesRealDirectories: false`. `/api/skills/governance/plans` stores dry-run
plans only. Plan approval changes plan status and audit rows, not Skill files.

## Phase 3B ownership

Phase 3B owns the write path:

- apply and verify operations;
- backup and rollback;
- atomic writes;
- directory locks;
- real lockfile writes;
- Runtime reload;
- stable Session-effective adapters when a Runtime exposes a session-bound
  Skill contract.

Until Phase 3B lands, governance plans are advisory and auditable only.

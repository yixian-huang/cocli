# MCP governance Phase 2A

Phase 2A adds durable desired state, deterministic dry-run planning, and an
auditable approval contract on top of the Phase 1 read-only inventory and
doctor. It intentionally stops before Runtime mutation.

## Ownership and profile model

An MCP profile is Runtime-neutral durable state. Every profile is versioned
and contains one or more desired server entries:

- stable server id, Runtime, alias, and optional redacted canonical definition;
- desired enabled state;
- sorted `allowTools` and `denyTools` policy;
- `approvalMode` and optional risk override;
- opaque `secretRefs` only.

Profiles are attached by reference to canonical cocli ownership boundaries:

- **machine:** the current `cocli_installation.installation_id`;
- **Workspace:** the portable `workspaces.id`;
- **Agent:** the persistent `agents.id`.

Effective desired state resolves in the fixed order `machine < workspace <
agent`. A higher-precedence entry replaces a lower-precedence entry for the
same `(runtime, serverId)`. Different entries from multiple profiles at the
same precedence are a conflict. Conflicted servers are omitted from effective
desired servers and produce a blocked manual plan action; cocli never uses
creation time, database row order, or last-write-wins as a tie-breaker.

Profile update/delete and binding delete require `expectedVersion`. A stale
version returns `409 Conflict`. Deleting a profile removes its bindings through
SQLite referential integrity; prior plans and decisions remain audit records.

## Secret boundary

Plaintext credentials are forbidden in profile definitions. Token-like args,
credential-bearing URLs, and invalid secret references are rejected before
persistence with a generic error that does not echo the submitted value.
Accepted references use an opaque `env://`, `keychain://`, `secret://`, or
`vault://` locator.

Inventory, profile, effective-state, plan, decision, error, log, and test
snapshot surfaces must not contain raw secret values. Plan before/after
summaries contain only a secret-reference count. Phase 2A does not read a
secret store, resolve references, authenticate accounts, or approve a Runtime.

## HTTP API

The machine-local API extends the existing `/api/runtimes/mcp` namespace:

| Method | Path | Contract |
|---|---|---|
| `GET` / `POST` | `/profiles` | List or create versioned profiles |
| `GET` / `PUT` / `DELETE` | `/profiles/:profileId` | Read, update, or version-guarded delete |
| `GET` / `POST` | `/bindings` | List or bind a profile to machine, Workspace, or Agent |
| `DELETE` | `/bindings/:bindingId` | Version-guarded unbind |
| `GET` | `/effective?workspaceId=&agentId=` | Explain effective desired state and conflicts |
| `POST` | `/plans` | Inspect current Phase 1 state and persist a dry-run plan |
| `GET` | `/plans/:planId` | Read a plan with current approval/staleness status |
| `POST` | `/plans/:planId/approve` | Record hash-bound future authorization |
| `POST` | `/plans/:planId/reject` | Record a reasoned rejection |

There is deliberately no `apply`, `write`, `reload`, `restart`, `oauth`, or
`authenticate` endpoint.

## Deterministic plan contract

Planning hashes the redacted effective desired state and a stable projection of
the latest Phase 1 observation. Collection timestamps are excluded, while
meaningful server definitions, independent observed states, schema hashes,
diagnostics, and evidence are included. Arrays and plan actions use explicit
stable ordering. The plan hash is SHA-256 over:

1. the base observation hash;
2. the effective configuration hash;
3. the sorted, redacted action list.

Every action reports Runtime, effective scope and target, server id and
fingerprint, before/after summaries, risk, reason, evidence, expected source
hash, and expected schema hash. Action kinds are:

- `add_configure`, `enable`, `disable`, `update`, and `remove`;
- `approval_required` and `authentication_required`;
- `manual_unsupported` for conflicts, unsupported Runtimes, missing evidence,
  or unknown state that prevents a trustworthy automated representation.

Removal, credential or approval boundaries, allowlist expansion, and profiles
whose names identify production/ops context are high or critical risk. A risk
override can raise risk but cannot lower the computed minimum. All Phase 2A
actions are previews; `dryRun` is always true and `applied` is always false.

## Approval and staleness

An approval records the exact plan hash, base observation hash, base config
hash, actor, decision time, and a required future expiry. Missing or already
expired deadlines are rejected; the desktop uses an explicit 15-minute
window. A rejection additionally requires a reason. Approval is accepted only
while the submitted plan hash and both current base hashes still match.

Plan reads re-evaluate the latest observation and effective desired-state
hashes. Observation drift, profile/binding drift, hash mismatch, or expiry
changes an earlier approval to `stale`/`expired`; the immutable decision remains
available for audit. The UI renders a valid approval as **approved but not
applied**.

## Phase 2B boundary

Phase 2B may consume this profile, action, hash, and decision contract when it
adds Runtime-specific writers. It must add explicit apply/reload/verify,
pre-write backup, rollback, stale-plan revalidation immediately before write,
and post-write native evidence. Phase 2B must not infer loaded, approved,
authenticated, healthy, or current-session-visible state from a config write.

Phase 2A does not edit user MCP files, invoke Runtime configuration commands,
approve OAuth or authentication, store plaintext secrets, or introduce a
Gateway/Registry.

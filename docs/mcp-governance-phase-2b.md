# MCP governance Phase 2B

Phase 2B turns a valid Phase 2A approval into a guarded local apply run. It
does not weaken the profile, deterministic-plan, redaction, or evidence
contract established by Phase 2A.

## Apply gate

`POST /api/runtimes/mcp/plans/:planId/apply` accepts the exact `planHash`,
`observationHash`, and `configHash`, an actor, and an explicit
`confirmHighRisk` flag. The server refuses pending, rejected, expired, stale,
or hash-mismatched approvals before any Runtime mutation. High/critical
actions require the extra confirmation.

Only non-blocked `add_configure`, `enable`, `disable`, `update`, and `remove`
actions enter a writer. `approval_required`, `authentication_required`, and
`manual_unsupported` actions are persisted as skipped/blocked and are never
executed. Repeating the same approved plan returns its durable run instead of
duplicating configuration.

## Runtime adapter and write safety

The runtime-neutral service exposes apply and rollback requests. The local
adapter currently writes only JSON MCP configuration shapes with behavior that
can be verified reliably (Cursor and Claude). Codex/Grok TOML and unsupported
policy fields remain structured manual results. This is an intentional safety
boundary, not a success fallback.

For every supported source the adapter:

1. rechecks the current inventory hash;
2. acquires an adjacent exclusive apply lock;
3. parses the current document and compares the planned definition
   fingerprint;
4. writes an opaque, checksummed backup before mutation;
5. changes only the MCP server subtree, preserving unrelated keys;
6. fsyncs a temporary file and atomically renames it over the source.

Tests invoke the writer only with temporary configuration roots. They never
write the machine's real Codex, Cursor, Claude, or Grok configuration.

## Secrets

Profiles and plans continue to persist only opaque secret references. The
apply boundary has no general secure resolver in this release, so any action
requiring `secretRefs` is blocked. Backup contents stay in files managed by the
runtime adapter; SQLite and API responses store only opaque backup metadata
and checksums. Errors and action reasons are fixed, redacted descriptions.

## Reload and verification

File-backed changes become available to new Runtime sessions. Phase 2B records
reload as `deferred` and never restarts an active session. After all isolated
writer actions complete, the service performs a fresh inventory/doctor pass
and compares configured/enabled state with the approved desired state. A
mismatch produces `mismatched` evidence and leaves rollback available; it is
never reported as successful application.

## Persistence and recovery

Apply runs, per-action status and reason, reload/verification summaries,
backup metadata, and rollback status are stored in the Phase 2B migration.
Partially successful Runtimes retain their own results. A later
`POST /api/runtimes/mcp/apply-runs/:runId/rollback` verifies backup checksums and
restores each source atomically (or removes a source created by the run).
Repeated reads and rollback requests are auditable and deterministic.

## Non-goals

Phase 2B does not introduce a Gateway/Registry, persist plaintext secrets,
approve OAuth/authentication, claim unsupported Runtime reload semantics, or
restart live sessions. Future adapter work may add reliable native CLI/API
writers and a secure secret resolver without changing the approval/hash gate.

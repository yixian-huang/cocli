# MCP governance Phase 2C

Phase 2C hardens Phase 2B apply with versioned adapter negotiation, preflight
proofs, a durable apply journal, and explicit recovery surfaces. It remains
local-only and never writes the machine's real MCP configuration during tests.

## Capability contract

`GET /api/runtimes/mcp/capabilities` returns a redacted capability snapshot
with a stable hash. Plans bind this `capabilityHash` alongside the observation
and desired-state hashes. Any adapter, binary version, schema, destination, or
operation-support drift makes the old plan and approval stale.

Each Runtime reports:

- read/discover, add/configure, enable/disable, remove, secret reference,
  reload, verify, and rollback support;
- `supported`, `read_only`, `unsupported`, or `unknown`;
- reason, evidence, binary path/version when known, config schema version,
  destination, allowed subtree, and reload strategy.

Current support matrix:

| Runtime | Write support | Reload | Notes |
| --- | --- | --- | --- |
| Codex | blocked unless a version-bound native CLI/app-server contract is proven | new-session-only | native probe/version evidence is reported; tests use fake binaries and isolated homes |
| Cursor | supported structured JSON fallback for `mcpServers` only | new-session-only | preserves unknown fields and rejects CAS/round-trip drift |
| Claude | supported structured JSON fallback for `mcpServers` only | new-session-only | same subtree and CAS limits as Cursor |
| Grok | read-only/manual | deferred | no stable transactional writer is assumed |

Secret-reference injection is unsupported unless an adapter has a proven
non-persistent injection channel. Opaque `env://` references can be resolved at
the execution boundary for tests, but resolved values are not serialized,
logged, or returned.

## Preflight and apply

`GET /api/runtimes/mcp/plans/:planId/preflight` re-probes capabilities and
inventory before apply. `POST /api/runtimes/mcp/plans/:planId/apply` still
requires a live, unexpired approval plus exact plan/config/observation hashes.
Phase 2C additionally requires the capability hash to match.

Only executable non-blocked actions may reach a writer. Manual, blocked,
auth-required, unsupported, and OAuth/login/restart actions remain skipped or
blocked. Commands are executed with argument arrays rather than through a
shell, and secret material is never placed in argv, API responses, SQLite, or
UI output.

## Journal and recovery

Apply runs persist a journal with idempotency keys, expected hashes, backup
references, attempt number, result, and redacted evidence. The state machine is:

`preflight -> locked -> backed_up -> written -> reload_pending -> reloaded -> verified`

Failure and recovery states are:

`failed`, `rolling_back`, `rolled_back`, and `recovery_required`.

On restart, an interrupted run can be inspected, resumed through the same apply
endpoint when hashes still match, rolled back when backups exist, or annotated
through `POST /api/runtimes/mcp/apply-runs/:runId/manual-recovery`. Completed
non-idempotent writes are not repeated when the journal already proves a
`written`, `reload_pending`, `reloaded`, or `verified` phase for the same
idempotency key.

Multi-Runtime apply uses saga-style partial results: one Runtime can verify
while another blocks or fails. The global status is partial/failed/blocked as
appropriate, and per-action status is retained.

## Reload and verification

Adapters report `native_reload`, `new_session_only`, `deferred`, or
`unsupported`. The current local adapters never terminate or restart active
Runtime sessions. Verification always uses a fresh inventory/doctor/native
readback pass. Discovery of configured files is not treated as proof that an
active session loaded the change; the UI reports `session_effective=unknown`
or `new_session_required` when that cannot be proven.

## Boundaries

Phase 2C does not introduce a Gateway/Registry, a secret store, production
restart automation, OAuth/auth approval, third-party script execution, or a
Grok writer. Tests use temporary HOME/config roots and fake binaries. The
machine's real Codex, Cursor, Claude, and Grok configuration paths are never
used as write targets by the test suite.

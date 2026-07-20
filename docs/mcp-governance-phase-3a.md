# MCP governance Phase 3A

Phase 3A packages MCP desired-state governance for review and migration, and
adds a library-only Runtime adapter SDK plus no-side-effect conformance suite.
It builds on Phase 2C capability/preflight/apply/recovery without introducing
a Gateway, Registry, Marketplace, dynamic plugin loader, or third-party code
execution.

## Portable governance bundles

`McpGovernanceBundle` is deterministic JSON with schema version
`2` (`MCP_GOVERNANCE_BUNDLE_SCHEMA_VERSION`). A bundle contains:

- profiles with desired servers, tool policy, approval mode, risk override,
  and opaque `secretRefs`;
- relative machine/Workspace/Agent bindings;
- optional capability expectations captured at export time;
- provenance (`producer`, source schema, profile fingerprints), `createdBy`,
  portability diagnostics, and a stable `contentHash`.

Bundles never contain secret plaintext, Runtime tokens, OAuth state, approval
records, apply runs, backup contents, active Session state, or absolute private
machine paths. Absolute command/argument/endpoint values are replaced by
`{{rebind:...}}` placeholders and marked as `machine_local`. Runtime
installations, binding targets, and secret references are always classified as
`requires_rebind`.

Supported API:

- `POST /api/runtimes/mcp/bundles/export-preview`
- `POST /api/runtimes/mcp/bundles/export`
- `POST /api/runtimes/mcp/bundles/import-preview`
- `GET /api/runtimes/mcp/bundles/imports`
- `GET /api/runtimes/mcp/bundles/imports/:auditId`
- `POST /api/runtimes/mcp/bundles/imports/:auditId/rebind`
- `POST /api/runtimes/mcp/bundles/imports/:auditId/commit`
- `POST /api/runtimes/mcp/bundles/imports/:auditId/cancel`

Export and preview are read-only. Import preview parses, validates, migrates,
hash-checks, and diagnoses the bundle, then records an audit row. Import commit
only creates or updates profiles and bindings. It never approves a plan and
never applies Runtime configuration.

## Safe import and rebinding

Import requires explicit rebinding for every relative target, Runtime
installation, secret reference, profile update target, and machine-local
placeholder. cocli never guesses by display name.

The preview reports:

- profile create/update/conflict operations;
- binding create or missing target rebinding;
- unsupported or missing Runtime installation;
- missing secret reference rebinding;
- capability expectation mismatch;
- blocked portability findings.

The commit path uses optimistic concurrency for profile updates. Repeating the
same bundle, actor, and rebindings returns the same preview audit; repeating a
completed commit returns the committed audit rather than creating duplicate
audit state. Capability expectations are advisory only and do not bypass live
Phase 2C preflight on the destination machine.

Schema migration is explicit. Version 1 bundles can be deterministically
migrated after their legacy content hash is verified. Future unknown versions,
unknown required fields, corrupted hashes, oversized bundles, and overly deep
documents fail closed with structured diagnostics.

## Adapter SDK and conformance

`mcp_adapter_sdk` defines the stable library boundary for first-party and
future third-party Runtime adapters:

- `McpRuntimeAdapter` trait: identity/version, capability probe, readback,
  preflight, apply action, reload, verify, rollback, and recovery.
- host-injected context/ports: config roots, allowed write roots, timestamps,
  and `McpSecretResolver`;
- redacted action requests and structured apply/write outcomes;
- recovery decisions and conformance reports;
- `FakeMcpAdapter` for offline tests.

Adapters do not receive Store or UI access. Secret resolution is a host
capability; the Phase 3A SDK exposes only the opaque reference and value digest
to the serializable contract, never the resolved value. Phase 3A does not load arbitrary
dynamic libraries, WASM, scripts, or remote packages.

The reusable conformance harness checks capability stability, evidence,
unsupported safe downgrade, canary redaction, write-root confinement, false
support claims, structured write evidence, reload/new-session-only boundaries,
verify output, and report hash stability. Tests run with temporary roots and
fake adapters. The `/api/runtimes/mcp/conformance` endpoint instead wraps the
production `RuntimeService` capability and inventory paths, runs only
observation-safe checks, and skips side-effect contracts. It never presents a
fake fixture as an actual Runtime adapter and does not replace live preflight.

## Boundaries

Phase 3A is portability plus SDK/conformance only. It does not:

- execute code embedded in a bundle;
- trust imported approvals;
- auto-apply imported desired state;
- provide a Registry, Gateway, Marketplace, or dynamic adapter loader;
- write outside explicitly rebound desired-state/profile/binding records;
- touch real Codex, Cursor, Claude, or Grok configuration during tests.

Phase 3B can build on this by adding signed bundle provenance, richer CLI
inspection surfaces, and documented third-party adapter packaging after the
extension trust model is complete.

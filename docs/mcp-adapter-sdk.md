# MCP Runtime adapter SDK and conformance kit

The Phase 3A adapter SDK is a library contract in `cocli-driver-core`. It lets
first-party adapters share one typed boundary without introducing dynamic
plugin loading, a Registry, a Marketplace, or execution of bundle-provided
code.

## Contract

`McpRuntimeAdapter` exposes:

- identity, adapter version, and SDK contract version;
- capability probe and redacted readback;
- action preflight and one-action apply outcomes;
- reload strategy, verification, rollback, and recovery decisions.

The host injects `McpAdapterSdkContext`, including temporary/config roots and
an explicit allowlist of writable roots. Adapters do not receive Store, API, or
UI handles. Commands and configuration mutations remain implementation details
of trusted built-in adapters and are still governed by Phase 2C CAS, locking,
backup, journal, reload, and verification rules.

`McpSecretResolver` is a host port. Its public result contains an opaque
reference and a digest, not a serializable plaintext value. A Runtime without a
proven non-persistent injection channel must continue to report secret
injection as unsupported or blocked.

## Conformance harness

`run_mcp_adapter_conformance` accepts an adapter, a host-owned isolated context,
and an explicit scenario. It produces a deterministic, machine-readable
`McpAdapterConformanceReport` and checks:

- stable capability/version output and evidence;
- safe unsupported downgrade;
- redaction of readback, preflight, apply, reload, verify, rollback, recovery,
  and final report output;
- write evidence for declared supported writes;
- confinement of declared write effects to host allowlisted roots;
- reload/new-session-only and session-effective boundaries;
- false capability claims and verify mismatch behavior.

The reusable `FakeMcpAdapter` is for offline, temporary-root contract testing
only. It exercises isolated file mutations for subtree preservation, CAS,
locking, atomic replacement, idempotency, filesystem confinement, recovery,
rollback, session boundaries, and verify mismatch.

The desktop endpoint does not rename those fakes as Codex, Cursor, Claude, or
Grok. It wraps capability and inventory observations returned by the production
`RuntimeService` and runs only no-write checks. Writer, reload, rollback, and
recovery cases are explicitly skipped there, so an observed report is not a
writer certification. Live Phase 2C preflight remains authoritative.

## Extension boundary

Phase 3A intentionally provides no dylib, WASM, script, remote package, or
third-party process loader. A future packaging phase must first define signing,
trust, sandboxing, lifecycle, and revocation. Until then, adapters are compiled
first-party implementations and the SDK is an interface plus test kit.

# MCP adapter SDK and conformance

The MCP adapter SDK is a library contract for Runtime-specific MCP adapters.
It is intentionally not a plugin loader. Phase 3A does not load arbitrary
dynamic libraries, WASM modules, shell scripts, or remote adapter packages.

## SDK boundary

Adapters implement `McpRuntimeAdapter` from `cocli-driver-core`:

- `identity` returns runtime, adapter name, adapter version, contract version,
  and redacted evidence.
- `probe_capabilities` reports the versioned Runtime capability matrix.
- `readback` returns redacted observed MCP state.
- `preflight_action` proves whether a plan action is executable.
- `apply_action` returns structured action/write effects.
- `reload` reports native reload, new-session-only, deferred, or unsupported.
- `verify` reports fresh readback comparison and session effectiveness.
- `rollback` restores host-provided backup descriptors.
- `recover` decides resume, rollback, manual recovery, or already-complete.

Adapters receive explicit context and ports. They do not access Store or UI
internals, and they do not own secret storage. `McpSecretResolver` is a host
boundary: resolved values may exist only in execution memory and must not be
serialized, logged, returned, or embedded in argv.

## Conformance harness

`run_mcp_adapter_conformance` executes a no-side-effect contract suite against
an adapter and a host-provided temporary context. The harness checks:

- identity and SDK contract version;
- deterministic capability probes and redacted evidence;
- safe unsupported downgrade;
- readback, preflight, apply, reload, and verify redaction;
- secret canary absence from reports;
- write effects confined to explicit allowed roots;
- supported write claims include write or backup evidence;
- unknown-field preservation, CAS, lock contention, atomic temporary cleanup,
  idempotency, and actual filesystem escape detection;
- crash-boundary recovery, rollback, reload/new-session-only,
  session-effective, and verify-mismatch boundaries;
- stable machine-readable conformance report hashes.

The included `FakeMcpAdapter` is the example adapter for tests. It can model
supported writes, unsupported actions, leaked canaries, false support claims,
and real out-of-root writes under a monitored temporary HOME. The online API
reports only first-party capability/readback observations obtained from the
production `RuntimeService`; it never labels fake fixtures as actual Runtime
adapters. Unexecuted online write/reload/recovery cases remain skipped. Live
Phase 2C preflight remains authoritative for real apply decisions.

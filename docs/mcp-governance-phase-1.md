# MCP governance Phase 1

Phase 1 provides a local, read-only MCP inventory and doctor for Codex,
Cursor, Claude, and Grok. It follows the existing Skills inventory/doctor
pattern while keeping the core model independent of any one Runtime.

## Read-only API

- `GET /api/runtimes/mcp/inventory` returns `servers`, `bindings`,
  `observations`, `diagnostics`, and the aggregate `observedAt` timestamp.
- `GET /api/runtimes/mcp/doctor` returns the same inventory plus a stable
  `summary` with status and error/warning counts.

Every observation has evidence and `observedAt`. The state fields are
independent and nullable where evidence is unavailable:

- `discoverable`: a definition or native candidate was found.
- `configured`: a supported configuration or native list contains the server.
- `loaded`: a Runtime-native surface reports the server loaded.
- `enabled`: the Runtime or binding enables the definition.
- `approved`: the Runtime approval boundary allows the server.
- `authenticated`: the Runtime reports usable authentication.
- `healthy` and `startup`: startup/health evidence.
- `currentSessionVisible`: an already-running session can see the server.
- `invoked`: invocation evidence exists.

`null` means unknown. In particular, configuration-file presence never sets
`loaded`, and a fresh CLI/app-server probe does not prove visibility in an
already-running session.

## Adapter boundary

`McpConfigAdapter` discovers and sanitizes definitions and desired bindings.
`McpRuntimeProbe` gathers read-only native evidence. Phase 1 intentionally has
no change-applier interface.

Native probes use the strongest local surface available and degrade to a
structured diagnostic:

- Codex: app-server/native status boundary with testable CLI JSON fallback.
- Cursor: configuration discovery plus `cursor-agent mcp list` and
  `cursor-agent mcp list-tools` when available.
- Claude: `claude mcp list` and per-server `claude mcp get` when available.
- Grok: `grok mcp list --json` and `grok mcp doctor --json`.

A missing binary, unsupported command, timeout, authorization failure, or
malformed response affects only that Runtime's evidence.

## Redaction and diagnostics

Canonical definitions are sanitized before aggregation. Suspected inline
secrets are replaced with redaction markers and represented only by a
`secretRefs` entry describing their location and kind. Raw command output is
not copied into responses or diagnostic messages.

The doctor reports duplicate endpoints/aliases, missing approval or
authentication, startup/health failure, desired-vs-observed configuration
drift, suspected plaintext secrets, and probe failures.

## Phase 2 boundary

Phase 2 may add explicit `plan -> approve -> apply -> verify` changes,
lockfiles, and drift remediation. It must preserve redaction and evidence
semantics. Phase 1 does not install a Gateway or Registry, write Runtime
configuration, manage secrets, approve servers, authenticate accounts, or
restart active Agent sessions.

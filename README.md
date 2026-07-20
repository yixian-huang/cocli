# cocli

> A local-first environment for persistent AI Agents and Channels.

cocli lets you keep durable Agents, talk to them directly, and bring several
Agents together in Channels for any kind of work: research, writing, analysis,
software development, operations, or a domain you define. Agent identity,
memory, tasks, and relationships survive the underlying CLI process and
runtime session.

**Status:** early alpha. The local server, SQLite state, web client, eight
Runtime adapters, durable delivery, Tasks, Memory, Skills, runtime history,
live execution events, search, state backup/restore, and Skill governance are
implemented. The persistent Agent/Channel model in
[DESIGN.md](DESIGN.md) is landed; APIs, Workspace provider contracts,
installers, and release guarantees are still evolving toward a public alpha.

## Product model

- **Agent** is a persistent worker identity. Runtime, model, Session, and CLI
  process are execution details beneath it.
- **Channel** is a durable collaboration context containing messages, Tasks,
  participating Agents, and shared context.
- **Workspace** is an optional scope/resource attachment. A directory, Git
  repository, worktree, document collection, or external resource can be a
  Workspace, but none is required to start.
- **Memory and Skills** are tools used by Agents. Runtime history and raw
  execution details are diagnostic surfaces for users.
- **Wiki is not part of the core product.** A future Wiki may be delivered as
  an optional plugin after the extension contract is stable.

See [DESIGN.md](DESIGN.md) for the canonical product, information architecture,
terminology, and non-goals.

## Run from source

Prerequisites: Rust 1.78+, Node.js 20+, npm, and at least one supported Agent
CLI if you want real Runtime execution.

```bash
cd web
npm ci
npm run build
cd ..
cargo run --bin cocli
```

Open <http://127.0.0.1:8090>. For deterministic local development without an
installed Agent CLI:

```bash
cargo run --bin cocli -- --fake-runtime
```

State is stored under the operating system's local application-data directory
unless `--data-dir` or `COCLI_DATA_DIR` is supplied. The HTTP listener is
loopback-only.

## Backup and restore

Create a transactionally consistent SQLite snapshot (the compatibility path):

```bash
cargo run --bin cocli -- --data-dir ./local-data backup --output ./cocli-backup.sqlite3
```

Create and preflight a portable bundle:

```bash
cargo run --bin cocli -- --data-dir ./local-data backup \
  --portable --output ./cocli-portable-backup
cargo run --bin cocli -- preflight --input ./cocli-portable-backup
```

The bundle directory contains `manifest.json`, a sanitized `state.sqlite3`,
and `checksums.json`. It excludes the installation identity, Bridge tokens,
OS credential references, and active execution state. Source-machine Workspace
bindings remain as non-current hints for explicit rebinding.

Restore and migrate a snapshot while the server is stopped:

```bash
cargo run --bin cocli -- --data-dir ./local-data restore --input ./cocli-backup.sqlite3
```

The same command accepts a portable bundle directory. Portable restore verifies
the manifest and SHA-256 checksums before staging or changing current state,
migrates and sanitizes the staged database, creates a fresh installation
identity, and leaves Workspaces unbound until explicitly rebound. It does not
merge active installations, resume source Runtime Sessions, or create/delete
Git worktrees.

Restore validates and migrates a staged copy before installing it. Existing
state is copied to `local-data/backups/pre-restore-*.sqlite3`, then the staged
database atomically replaces the live database; a failed replacement leaves
the live database in place. Runtime Workspace files are separate from the
SQLite snapshot and must be copied independently when needed.

## Supported Runtime adapters

The first-party adapter matrix is Claude, Cursor, Codex, Gemini, Kimi, Grok,
Chatrs, and OpenCode. Availability and capabilities are discovered locally;
not every Runtime exposes the same model, skill, cancellation, steering, or
session-resume features.

## Desktop Skill governance

Skill governance is available as an Agent/Runtime governance surface.
The desktop Skills workspace keeps the existing local library and per-Agent
install/uninstall flow, and adds Runtime inventory, doctor details, versioned
desired-state profiles, lockfile previews, drift classification, deterministic
plans, approved apply/recovery, canonical scope inspection, managed artifacts,
materializations, adoption, workspace lockfile restore, and reference-gated
garbage collection.

The inventory API exposes machine-level endpoints at
`/api/runtimes/skills/{inventory,doctor}` and Agent-level endpoints at
`/api/agents/:agent_id/skills/{inventory,doctor}`. The governance API is under
`/api/skills/governance`.

Evidence boundaries are intentionally strict. Filesystem discovery, Codex
app-server `skills/list`, Grok `inspect --json`, and Cursor filesystem
inventory do not prove that a concrete running Session loaded or activated a
Skill. `sessionEffective` remains `unknown` unless a future session-bound
native contract provides direct proof. The current Cursor CLI has no stable
read-only Skill listing or session Skill contract, so cocli performs a bounded
capability check, returns structured unsupported/manual governance evidence,
and falls back to filesystem inventory.

Governance persists profiles, profile bindings, immutable lock snapshots,
dry-run plans, approval audit rows, apply runs, action journals, scoped locks,
backup references, quarantine references, canonical managed artifacts,
per-target materializations, workspace lockfile records, GC references, and
recovery state in SQLite. Mutable profile, binding, and plan decisions use
optimistic `expectedVersion` checks. Apply requires a non-stale approved plan,
matching observation/desired/lock hashes, an idempotency key, and a current
confirmation nonce for high-risk actions.

The governed apply path is intentionally local-only. For machine/user,
workspace/project, and Agent scopes, it can automatically copy or symlink
digest-verified local, cocli-managed, library, or vendored artifacts into a
Runtime-derived supported root. It records immutable artifacts and per-Skill
materializations, writes `.cocli/skills.lock.json` for Workspace plans through
CAS, backup, fsync, atomic rename, journal, and rollback boundaries, and removes
only hash-matched managed/adopted entries through quarantine. Machine and
Workspace writes are high risk and require the current preview-bound
confirmation nonce. It does not accept arbitrary target paths, execute Skill
scripts, clone repositories, resolve private credentials, download from a
Registry or Marketplace, restart Runtime Sessions, or claim Session activation.
Filesystem/runtime verification reports installed or configured-on-disk state;
`sessionEffective` remains `unknown` without session-bound native evidence.
The Phase 3C scope contract distinguishes machine/user, workspace/project, and
Agent targets; records runtime-specific and shared Skill roots; stores
immutable cocli-owned artifacts separately from Runtime search paths; and tracks
each per-Skill materialization as `managed`, `adopted`, `unmanaged`, or
`foreign`. Adoption supports audited record-only ownership, import-copy into the
managed store, or an explicit keep-foreign record. Preview hashes and
idempotency-bound confirmation nonces protect every managed-store, adoption,
restore, and GC commit. Whole-root symlink takeover is blocked, while GC uses
fresh references, optimistic versions, fingerprints, and quarantine for managed
artifact bytes.

See [docs/skill-governance-phase-3a.md](docs/skill-governance-phase-3a.md)
for desired-state and dry-run planning,
[docs/skill-governance-phase-3b.md](docs/skill-governance-phase-3b.md) for
approved apply, verification, rollback, and recovery semantics, and
[docs/skill-governance-phase-3c.md](docs/skill-governance-phase-3c.md) for
canonical scope, materialization, lockfile, adoption, and GC contracts.

## Desktop MCP governance

MCP governance Phase 1 provides read-only observation. The local API exposes cross-Runtime
inventory and doctor results at `/api/runtimes/mcp/inventory` and
`/api/runtimes/mcp/doctor`, and the desktop MCP workspace renders the same
Runtime × Server evidence matrix. Configuration discovery and Runtime-native
probing are separate adapters: a discovered or configured server is not
reported as loaded, approved, authenticated, healthy, visible to a current
session, or invoked unless the corresponding evidence exists.

The inventory stores endpoint fingerprints and redacted canonical definitions;
secret values are never returned. Missing CLIs, unsupported commands, timeouts,
invalid output, approval/authentication gaps, startup failures, duplicate
endpoints/aliases, and configuration drift are structured diagnostics. One
Runtime probe failing does not fail the aggregate request. See
[docs/mcp-governance-phase-1.md](docs/mcp-governance-phase-1.md) for the API
contract and the explicit Phase 2 boundary.

Phase 2A adds durable, versioned MCP profiles and machine/Workspace/Agent
bindings, deterministic effective desired-state resolution, stable dry-run
plans, and hash-bound approve/reject records. Profile inheritance is fixed at
`machine < workspace < agent`; conflicting profiles at the same precedence are
reported instead of silently overwritten. Plans compare the latest Phase 1
observation with desired state, preserve evidence, mark risky or unsupported
work, and contain no apply capability. The desktop exposes Profiles and Plan
Preview alongside the existing matrix and labels approvals as “approved but
not applied”. See [docs/mcp-governance-phase-2a.md](docs/mcp-governance-phase-2a.md).

Phase 2A never writes Codex, Cursor, Claude, or Grok configuration, never
performs OAuth/authentication or Runtime approval, and accepts secret
references only.

Phase 2B adds an explicitly confirmed apply flow for a still-current approval.
The API rechecks the plan, desired configuration, observation hashes, and
expiry immediately before dispatch. Supported Cursor/Claude JSON adapters use
per-source locks, compare-and-swap checks, pre-write backups, and atomic
`mcpServers` subtree updates while preserving unrelated user configuration.
Codex/Grok TOML, tool-policy, authentication, and unresolved secret-reference
actions return structured blocked/manual results instead of pretending to
succeed. Active sessions are not restarted: reload is recorded as deferred,
then a fresh inventory verifies desired state. Apply runs, per-action outcomes,
backups, verification, and rollback remain durable and auditable. See
[docs/mcp-governance-phase-2b.md](docs/mcp-governance-phase-2b.md).

Phase 2C hardens apply with a versioned Runtime capability contract and durable
recovery journal. Plans now bind the adapter capability hash in addition to
observation and desired-state hashes; adapter or binary-version drift makes an
approval stale. The API exposes `/api/runtimes/mcp/capabilities` and
`/api/runtimes/mcp/plans/:plan_id/preflight` so the desktop can show each
Runtime's read, write, secret, reload, verify, and rollback support before
apply. Codex capability negotiation is native-CLI/version aware, Cursor and
Claude keep controlled JSON fallback writers for the MCP subtree, and Grok
remains read-only/manual until a transactionally safe writer exists. Apply
runs persist a journal across preflight, lock, backup, write, reload, verify,
failure, rollback, and recovery-required phases; resumed runs use idempotency
keys and do not repeat completed non-idempotent writes. Reload remains
new-session-only/deferred and never restarts active sessions. See
[docs/mcp-governance-phase-2c.md](docs/mcp-governance-phase-2c.md).

Phase 3A adds portable MCP governance bundles plus a library-only adapter SDK
and conformance harness. Bundles export versioned, deterministic desired state
with relative bindings, opaque secret references, provenance, optional
capability expectations, portability diagnostics, and a stable content hash.
Import is preview-first and requires explicit rebinding for machine,
Workspace, Agent, Runtime, secret, and machine-local values; commit only writes
profiles/bindings with optimistic concurrency and never imports approvals or
applies Runtime configuration. The SDK defines the redacted adapter boundary
and no-side-effect conformance checks for capability evidence, unsupported
downgrade, CAS/write confinement, secret canary redaction, reload/verify, and
recovery. See [docs/mcp-governance-phase-3a.md](docs/mcp-governance-phase-3a.md).
The library boundary and reusable test-kit contract are documented separately
in [docs/mcp-adapter-sdk.md](docs/mcp-adapter-sdk.md) and
[docs/mcp-adapter-sdk-conformance.md](docs/mcp-adapter-sdk-conformance.md).

## Skill and MCP governance integration

Skill and MCP governance share one API process, Store, migration sequence, and
desktop navigation while remaining separate supporting capabilities. MCP owns
migrations 0013-0016 and Skill governance owns 0017-0019. Databases created by
the isolated Skill development lineage, where those Skill migrations were
temporarily recorded as 0013-0015, are reconciled transactionally by exact
migration name before MCP migrations run. Profiles, approvals, journals,
locks, recovery scans, bundles, artifacts, materializations, and lockfiles stay
in domain-specific tables and namespaces.

See [docs/governance-integration.md](docs/governance-integration.md) for the
merge history, migration compatibility contract, unified validation matrix,
and explicit unsupported boundaries.

## Repository layout

- `bin/cocli/` — local server binary
- `bin/cocli-bridge/` — capability-scoped Agent collaboration bridge
- `crates/cocli-store/` — SQLite durable state and migrations
- `crates/cocli-api/` — local HTTP API and delivery coordination
- `crates/cocli-server/` — server assembly and local Runtime integration
- `crates/cocli-agent/` — Runtime-neutral Agent lifecycle and prompt contract
- `crates/cocli-driver-*` — first-party Runtime adapters
- `web/` — local React client
- `shared/` — shared TypeScript protocol types

## Development checks

```bash
cargo test --workspace --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo +stable fmt --all -- --check
cargo build --workspace --locked
cd web
npm test
npm run lint
npm run build
```

## Relationship to cocli cloud

This repository is the source of truth for the reusable local Runtime/Driver
layer. cocli cloud may consume exact OSS revisions while keeping tenant,
billing, quota, remote-connection, and hosted operations code outside this
repository. See [docs/runtime-ownership.md](docs/runtime-ownership.md).

## License

Dual MIT / Apache-2.0. See [LICENSE](LICENSE), [LICENSE-MIT](LICENSE-MIT), and
[LICENSE-APACHE](LICENSE-APACHE). The cocli name and marks are covered by
[TRADEMARK.md](TRADEMARK.md).

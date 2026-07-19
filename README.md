# cocli

> A local-first environment for persistent AI Agents and Channels.

cocli lets you keep durable Agents, talk to them directly, and bring several
Agents together in Channels for any kind of work: research, writing, analysis,
software development, operations, or a domain you define. Agent identity,
memory, tasks, and relationships survive the underlying CLI process and
runtime session.

**Status:** early alpha. The local server, SQLite state, web client, eight
Runtime adapters, durable delivery, Tasks, Memory, Skills, runtime history,
live execution events, search, and state backup/restore are implemented. The
persistent Agent/Channel model in [DESIGN.md](DESIGN.md) is landed; APIs,
Workspace provider contracts, installers, and release guarantees are still
evolving toward a public alpha.

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

Create a transactionally consistent SQLite snapshot:

```bash
cargo run --bin cocli -- --data-dir ./local-data backup --output ./cocli-backup.sqlite3
```

Restore and migrate a snapshot while the server is stopped:

```bash
cargo run --bin cocli -- --data-dir ./local-data restore --input ./cocli-backup.sqlite3
```

Restore validates and migrates a staged copy before installing it. Existing
state is moved to `local-data/backups/pre-restore-*.sqlite3`, making the
operation recoverable. Runtime Workspace files are separate from the SQLite
snapshot and must be copied independently when needed.

## Supported Runtime adapters

The first-party adapter matrix is Claude, Cursor, Codex, Gemini, Kimi, Grok,
Chatrs, and OpenCode. Availability and capabilities are discovered locally;
not every Runtime exposes the same model, skill, cancellation, steering, or
session-resume features.

## Desktop Skill governance

The first Skill governance phase is available as a supporting Agent/Runtime
diagnostic surface. The desktop Skills workspace keeps the existing local
library and per-Agent install/uninstall flow, and adds a read-only
Runtime × Skill inventory plus doctor details. The HTTP API exposes matching
machine-level endpoints at `/api/runtimes/skills/{inventory,doctor}` and
Agent-level endpoints at `/api/agents/:agent_id/skills/{inventory,doctor}`.

Discovery reports runtime compatibility, ordered search paths, scope, source
and resolved paths, managed/external/broken state, invalid frontmatter,
broken symbolic links, duplicate targets, and shadowed names. Cursor Agent
Skills are discovered from `.cursor/skills` and `.agents/skills` at workspace
and user scope; `.cursor/rules` remains a separate Rules surface. Claude,
Codex, and Grok retain their existing search-path behavior.

The Codex and Grok drivers now augment that filesystem scan with read-only
native evidence from app-server `skills/list` and `grok inspect --json`.
Inventory distinguishes a filesystem-only installed candidate from a Skill
returned by the Runtime, exposes Runtime-reported disabled state when present,
and falls back to filesystem evidence with a doctor warning when a native probe
fails. A native discovery response is still not proof that an already-running
Runtime Session loaded or activated the Skill. The doctor UI and API expose the
evidence source explicitly, and discovery does not write to user-global Skill
directories. Planned follow-up work adds a Cursor native probe,
plan/apply/verify changes, and lockfile/drift governance.

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
cargo test --workspace
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo fmt --all -- --check
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

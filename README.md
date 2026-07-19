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

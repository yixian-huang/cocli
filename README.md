# cocli

> A local-first environment for persistent AI Agents and Channels.

cocli lets you keep durable Agents, talk to them directly, and bring several
Agents together in Channels for any kind of work: research, writing, analysis,
software development, operations, or a domain you define. Agent identity,
memory, tasks, and relationships survive the underlying CLI process and
runtime session.

**Status:** early alpha. The local server, SQLite state, web client, eight
Runtime adapters, durable delivery, Tasks, Memory, Skills, runtime history,
live execution events, search, state backup/restore, and read-only Skill
governance are implemented. The persistent Agent/Channel model in
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

Skill governance is available as a supporting Agent/Runtime diagnostic surface.
The desktop Skills workspace keeps the existing local library and per-Agent
install/uninstall flow, and adds read-only Runtime inventory, doctor details,
versioned desired-state profiles, lockfile previews, drift classification, and
dry-run governance plans.

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

Phase 3A governance is read-only for runtime and filesystem Skill directories.
It persists profiles, profile bindings, immutable lock snapshots, dry-run
plans, and approval audit rows in SQLite; uses optimistic `expectedVersion`
checks for mutable profile, binding, and plan decisions; computes stable
SHA-256 hashes over deterministic observations, desired state, lock previews,
and plans; and marks approvals stale when observation, desired-config, or lock
hashes change.

Phase 3A does not apply changes, write lockfiles into workspaces, write runtime
or global Skill directories, download or execute scripts, mutate app/runtime
configuration, reload runtimes, or claim Session activation. Phase 3B owns
apply/verify, backup/rollback, atomic writes, directory locks, runtime reload,
and real session-effective adapters when stable Runtime contracts exist. See
[docs/skill-governance-phase-3a.md](docs/skill-governance-phase-3a.md).

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

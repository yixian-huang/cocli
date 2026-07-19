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
directories.

Inventory and doctor responses include `observedAt`, `cacheStatus`, and a
three-second `expiresAt` boundary. Concurrent requests for the same Runtime or
Agent snapshot share one in-flight probe; a result may be reused only inside
that short TTL. Pass `?force=true` to inventory or doctor endpoints to bypass
an older cached result. Skill install, uninstall, and reinstall invalidate the
affected Agent snapshot. Ordinary `GET /api/agents/:agent_id/skills` remains a
filesystem-only compatibility path and does not wait for a native probe.

Machine inventory now scans each supported Runtime's user/global roots even
when no Agent exists, then overlays Agent workspace results. Stable Skill and
issue fingerprints deduplicate aliases and repeated machine/Agent evidence.
Per-Runtime and per-Agent failures are returned as structured `diagnostics`
alongside successful results rather than failing the whole machine report.

Evidence boundaries are intentionally strict:

- `machine-discovered` means a candidate was observed in a Runtime user/global
  search root or its read-only native discovery response.
- `runtime-discovered` means a Runtime native probe returned the candidate for
  the probed working directory.
- `agent-workspace` means the candidate exists in that Agent's workspace scope.
- `session-effective` means an active Session demonstrably loaded or activated
  the Skill. Codex/Grok discovery does not currently prove this, so it remains
  unknown unless future Session-native evidence says otherwise.

Planned follow-up work is limited to a Cursor native probe, Session-effective
evidence, plan/apply/verify changes, and lockfile/drift governance.

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

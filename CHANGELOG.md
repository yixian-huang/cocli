# Changelog

All notable changes to this project will be documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Early public-alpha product since the M0 bootstrap (`0.0.0`). The local server,
durable Agent/Channel model, portable recovery, local Skill/MCP governance, and
conversation-first Channel chrome are in tree. Workspace version stays `0.0.0`
until the release tag. This section does **not** claim signed installers or
prebuilt GitHub Release archives.

### Added

- Local loopback server with SQLite durable state, embedded web UI, eight
  Runtime adapters discovered on PATH, `--fake-runtime` for deterministic
  development, and durable Channel/Agent message delivery.
- Persistent Agents and Channels as first-class subjects: many-to-many
  membership, direct Agent conversation (hidden compatibility Channels),
  capability-scoped Agent self-organization, and audited idempotency-keyed
  operations with an operation-history API.
- Portable backup, preflight, and restore (`backup --portable`, `preflight`,
  `restore`) for versioned `cocli-portable-backup` bundles: checksummed
  `manifest.json` plus sanitized `state.sqlite3`, schema 12 → 19 migration,
  a fresh installation identity, and fail-closed replay of pre-restore Skill
  or MCP apply. Desired-state Skill/MCP profiles and bindings survive;
  installation row, Bridge tokens, working-state, in-flight deliveries, live
  sessions, and binding secrets do not.
- `cocli rebind --workspace-id --local-locator` to bind a Workspace resource
  handle after portable restore without starting the HTTP server. Run only
  while the server is stopped. This binds a resource handle, not a Git product
  workflow.
- Local Skill and MCP governance: inventory and doctor, versioned desired-state
  profiles and bindings, hash-bound dry-run plans and approvals, and a safe
  local apply/verify/rollback loop. Remote sources, install scripts, Runtime
  reload, and session-effective proof are not shipped.
- Global search, downloadable consistent SQLite snapshots, live execution event
  delivery, and reconnect-aware client state refresh.
- Persisted Agent descriptions/instructions and Channel descriptions; Runtime
  prompts receive durable Agent instructions.
- Subject-first Channel and Agent navigation, direct Agent conversation,
  membership-aware shared Memory, lifecycle controls, and optional Workspace
  attachments for both subject types.
- `DESIGN.md` as the canonical product and interaction contract, and
  `docs/SUPPORT_MATRIX.md` as the honest alpha support surface.
- User-scoped `scripts/install.sh` / `scripts/install.ps1` (unsigned SHA-256;
  `COCLI_ARTIFACT_DIR` until GitHub Releases exist) and
  `scripts/smoke-installed.sh`.

### Fixed

- First-install rollback in `scripts/install.sh` / `scripts/install.ps1`: if
  the second binary fails, remove the newly installed first binary instead of
  leaving a lone `cocli`. Upgrade restore from `*.prev` is unchanged.

### Changed

- Established persistent Agents and Channels as cocli's two first-class
  product subjects; Project and Git workflows are optional Workspace providers
  (thin resource handles only).
- Migrated Agents from single-Channel ownership to many-to-many Channel
  membership and direct Agent conversation while hiding compatibility Channels.
- Reframed Runtime, Session, Turn, and CLI state as execution diagnostics
  beneath the persistent Agent identity.
- Generalized the base Agent contract beyond software development.
- Moved Tasks and shared context beneath Channels and Memory, Skills,
  Workspace, and diagnostics beneath their owning subject.
- Channel chrome is conversation-first: Conversation, Members, and Memory are
  the primary surfaces; Tasks are optional coordination disclosure, not a peer
  tab. Empty Channels invite a first message or an Agent invite, not a Task
  board or Goal field. Bridge Task APIs are unchanged.

### Removed

- Removed Wiki from the core product surface and Agent tool contract. Wiki may
  return later as an optional plugin after the extension contract is stable.
- Removed the legacy Wiki-backed Memory implementation after migrating durable
  memory into its own storage table.

### Notes

- Public alpha remains `0.0.x`. APIs, schemas, and UI can break between
  commits. Install from source (`cargo run --bin cocli`) or
  `scripts/install.sh` with `COCLI_ARTIFACT_DIR` until GitHub Release archives
  exist. Signed installers and prebuilt archives are not claimed.

## [0.0.0] — 2026-05-21

M0 bootstrap complete. Workspace skeleton, daemon-rs sources imported,
web cherry-picked, governance + CI in place. Repo pushed to GitHub
(private). No runtime behavior yet — `cocli --version` is the only
working command.

Next: M0.0.1 (channels + messages, no agent yet).

### Added
- Workspace skeleton (15 crates declared in Cargo.toml; 8 imported from
  the upstream daemon-rs sources; 7 new placeholders + 1 binary stub)
- web/ React frontend cherry-picked from upstream (with shared/ co-dependency)
- Governance files (LICENSE × 3, TRADEMARK, CONTRIBUTING, CODE_OF_CONDUCT,
  SECURITY, GOVERNANCE)
- CI workflows for 5 platforms (Linux x64/ARM, macOS Intel/Apple Silicon,
  Windows x64) — Section 6 of M0
- DCO check on PRs
- Issue + PR templates

### Notes
- This is M0 bootstrap — no runtime behavior yet. `cocli` binary prints
  a version string and exits.

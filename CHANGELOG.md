# Changelog

All notable changes to this project will be documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Established persistent Agents and Channels as cocli's two first-class
  product subjects; Project and Git workflows are optional Workspace providers.
- Migrated Agents from single-Channel ownership to many-to-many Channel
  membership and direct Agent conversation while hiding compatibility Channels.
- Added capability-scoped Agent self-organization contracts for creating and
  listing Agents and Channels and managing Channel membership.
- Reframed Runtime, Session, Turn, and CLI state as execution diagnostics beneath
  the persistent Agent identity.
- Generalized the base Agent contract beyond software development.
- Moved Tasks and shared context beneath Channels and Memory, Skills, Workspace,
  and diagnostics beneath their owning subject.

### Removed

- Removed Wiki from the core product surface and Agent tool contract. Wiki may
  return later as an optional plugin after the extension contract is stable.
- Removed the legacy Wiki-backed Memory implementation after migrating durable
  memory into its own storage table.

### Added

- Added `DESIGN.md` as the canonical product and interaction contract.
- Added global search, downloadable consistent SQLite state snapshots, and
  recoverable offline backup/restore commands.
- Added live execution event delivery and reconnect-aware client state refresh.
- Persisted Agent descriptions/instructions and Channel descriptions/goals;
  Runtime prompts now receive durable Agent instructions.
- Added audited, idempotency-keyed Agent self-organization operations and an
  Agent operation-history API.
- Added subject-first Channel and Agent navigation, direct Agent tasks,
  membership-aware shared Memory, lifecycle controls, and optional Workspace
  attachments for both subject types.

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

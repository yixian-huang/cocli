---
title: Portable Backup and Rebinding Session Log
category: execution-log
tags: [self-bootstrap, dogfood, workspace, backup, rebinding]
updated: 2026-07-18
---

# Portable Backup and Rebinding Session Log

## Persistent execution goal

Complete cocli-native Portable Backup & Rebinding while preserving the
canonical Workspace descriptor boundary, legacy compatibility, staged restore
safety, and cross-installation rebinding evidence.

## cocli-native dogfood control-plane

- Data directory: `/Users/yixian.huang/.local/share/cocli-bootstrap-m2`
- Listener: `http://127.0.0.1:55714`
- Channel `cocli-bootstrap`: `6f3440ac-1915-4da4-87f8-46cf674a9fd7`
- Workspace Agent: `7c31d984-f4e2-419f-b29a-82ebb7914148`
- Portability Agent: `55e920d1-d924-477d-b062-59aea390ceaf`
- Verification Agent: `4f985448-dcbe-4f6e-a3b2-779d9bf26f1e`
- Git Workspace: `6743521f-c977-4ea6-a53c-4df996e9cb0c`
- Installation at bootstrap: `79de7fc9-0c51-4812-87f1-ec909955d6f2`

The first server process used the pre-existing debug binary, which could write
the durable Channel, Agents, and Tasks but predated the Workspace routes. The
current branch binary was then built and restarted on the same data directory;
the durable control-plane records survived and the Workspace was created and
verified through the current API.

## Durable task graph

1. `Canonical Workspace descriptor and compat DTO boundary`
   - Task id: `6d17fa39-7913-4690-8fe9-b85b74820b09`
   - Owner: Workspace Agent
2. `Versioned portable backup bundle, preflight, sanitization, staged restore,
   and rebind`
   - Task id: `ad75f0fe-c254-4dbf-9c91-ec834aa0477c`
   - Owner: Portability Agent
   - Depends on task 1
3. `Cross-data-dir and moved Git path migration verification plus full quality
   gates`
   - Task id: `e9ee0aae-b049-40c4-87dc-0d37d74f8830`
   - Owner: Verification Agent
   - Depends on tasks 1 and 2

All three tasks were claimed successfully. Tasks 1 and 2 reached `done`; task
3 entered `in_review` while the final quality gates and independent reviews
ran, then reached `done` after both reviews cleared.

## Workspace identity and binding evidence

- Provider: `git`
- Portable locator: `https://github.com/yixian-huang/cocli.git`
- Preferred ref hint: `codex/local-platform-goal`
- Current-machine binding: `/Users/yixian.huang/code/cocli`
- Verification state: `ready`
- Capabilities: `filesystem=true`, `git=true`
- Verification error: none

The canonical Git remote is the portable identity. The absolute checkout path
is present only in the installation binding after the descriptor update and
explicit rebind.

## Runtime and approval boundary

The three durable responsibility Agents use cocli's deterministic fake Runtime
for registration and control-plane evidence only. No Runtime sandbox or
approval mechanism is bypassed. This Codex task performs repository edits,
tests, reviews, and Git inspection; cocli remains the durable control-plane and
ledger until a real Runtime execution is explicitly configured and approved.

## Verification status

- Channel creation: passed
- Agent creation/registration: passed
- Task claim and dependency graph: passed
- Git Workspace descriptor update: passed
- Explicit current-installation rebind: passed
- Workspace verification: `ready`
- Portable bundle and cross-installation migration verification: passed

## Cross-installation migration evidence

- Bundle: `/Users/yixian.huang/.local/share/cocli-bootstrap-m2-bundle`
- Bundle format/version: `cocli-portable-backup` / `1`
- Inventory version: `1` (manifests created before the field was added default
  to version 1 and still pass current preflight)
- Schema version: `12`
- Snapshot SHA-256:
  `127f80abe946ceb916a87a57ddb053cbacbf5c9077643126d8d3446cf67d79cd`
- Manifest inventory: 1 Channel, 3 Agents, 3 Tasks, 1 Workspace,
  1 attachment, 1 source binding hint, Provider `git`, Runtime `fake`
- Restored data directory:
  `/Users/yixian.huang/.local/share/cocli-bootstrap-m2-restored`
- Restored listener: `http://127.0.0.1:55715`
- Fresh installation id: `75bc80e6-8a98-4ba7-8497-d01e2789ac32`
- Moved Git binding:
  `/Users/yixian.huang/.local/share/cocli-bootstrap-m2-moved-repo`
- Moved Git remote: `https://github.com/yixian-huang/cocli.git`
- Rebound/verified state: `ready`

Preflight completed before restore and reported the source binding as a hint.
Before rebind, the restored installation synthesized an `unbound` current
binding while retaining the source installation/path record. After explicit
rebind, both the new ready binding and the source hint were readable.

The restored database retained all durable subject and Task ids. It contained
zero Bridge tokens, working-state rows, active Runtime Sessions, running Agent
states, and binding secret references. The source database retained its
original installation id and three Bridge tokens, proving sanitization was
applied only to the exported state.

## Final implementation and quality evidence

- Canonical Workspace APIs expose the descriptor DTO only. Legacy
  Agent/Channel attach/list routes convert through an explicit
  `LegacyWorkspace` compatibility response.
- Git portable identity is the canonical remote; local absolute checkout paths
  are installation bindings. Directory portable locators reject absolute local
  paths at the store boundary.
- Restore rechecks the SHA-256 of the staged copy before opening it. Invalid or
  changed bundle bytes cannot reach migration or installation.
- Existing state stays at the live path until installation succeeds. Unix uses
  a synced safety copy plus same-directory atomic rename; Windows uses
  `ReplaceFileW` with write-through and an atomic safety backup.
- Unknown Provider descriptors remain readable and are reported unavailable;
  Managed materialization remains explicitly outside this milestone.

Fresh final gates on the completed diff:

- Focused portable/restore tests: 6 passed.
- `cargo test --workspace --locked`: passed. One pre-existing concurrent
  delivery test was transient on the first run; its exact rerun and the full
  workspace rerun both passed.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`:
  passed.
- `npm test`: 57 files passed, 1 skipped; 220 tests passed, 1 skipped.
- `npm run lint` and `npm run build`: passed.
- `cargo +stable fmt --all -- --check` and `git diff --check`: passed.
- Independent code review: `APPROVE`, zero remaining issues.
- Independent architecture review: `CLEAR`, no unresolved blocker.

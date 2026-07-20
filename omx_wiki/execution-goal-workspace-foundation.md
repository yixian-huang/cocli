---
title: Execution Goal — Portable Workspace Foundation
category: decision
tags: [goal, workspace, provider, migration, self-bootstrap]
updated: 2026-07-18
---

# Execution Goal — Portable Workspace Foundation

## Goal

Establish the portable Workspace foundation that cocli will use to complete its
own A4-A6 work: separate logical Workspace identity, Agent/Channel attachment,
and installation-specific binding; migrate existing records safely; expose the
minimum Store and HTTP contracts; and prove the behavior with focused tests.

This is the first milestone of [[cocli-self-bootstrap]], not the entire public
alpha program.

## Required reading

- `DESIGN.md`
- `ROADMAP.md`
- [[cocli-self-bootstrap]]
- [[workspace-provider-portability]]

## Current implementation facts

- `crates/cocli-store/migrations/0009_agent_channel_ontology.sql` created a
  single `workspaces` table with inline owner, kind, locator, and metadata.
- `crates/cocli-store/src/lib.rs` exposes `Workspace`, `attach_workspace`,
  `list_workspaces`, and `get_workspace`.
- `crates/cocli-api/src/lib.rs` exposes Agent/Channel/Bridge attach/list routes.
- The repository has substantial uncommitted subject-model changes. Preserve
  them, inspect before editing, and do not revert unrelated work.
- Migrations 0009-0011 already exist; allocate the next migration number only
  after confirming the current filesystem state.

## Scope

1. Define persisted Workspace, SubjectWorkspace attachment, and
   WorkspaceBinding types with typed provider and binding states.
2. Add a forward migration from the inline-owner model without losing existing
   records.
3. Preserve compatibility for existing Agent/Channel attach/list callers while
   adding explicit Workspace read/update/detach and binding operations.
4. Introduce an internal Provider boundary sufficient for validation and
   resolution, without publishing a third-party plugin ABI.
5. Implement minimal Directory and Git validation for existing local resources.
6. Return structured unavailable/needs-attention errors without making
   Agent/Channel reads fail.
7. Add Store and API integration tests for migration, sharing, rebind, detach,
   missing paths, and unknown Provider preservation.

## Non-goals for this milestone

- Portable backup bundle implementation.
- Cross-OS UI wizard.
- Automatic Git clone or worktree creation.
- Destructive worktree cleanup.
- Signing, installers, or release publication.
- Public Provider SDK or plugin ABI.
- Agent-owned Diff, checkpoint, rollback, or validation policy.

## Acceptance criteria

1. One logical Workspace can attach to both an Agent and a Channel.
2. Deleting one attachment does not delete the Workspace or external data.
3. A Directory or Git Workspace can have different bindings for different
   installation identifiers.
4. Restored/imported source-machine bindings are never silently selected as the
   current machine binding.
5. A missing path produces a recoverable binding state and does not block
   reading the owning subject.
6. Existing inline-owner Workspace rows migrate without losing kind, locator,
   metadata, owner, or timestamps.
7. Unknown provider descriptors round-trip without data loss.
8. Focused Store/API tests pass, followed by workspace Rust tests, clippy, and
   the relevant web typecheck/build if shared API types change.

## Verification sequence

1. Lock migration and compatibility behavior with tests.
2. Implement storage types and queries.
3. Implement API compatibility and new binding routes.
4. Run focused Store tests.
5. Run focused API integration tests.
6. Run `cargo clippy` for changed crates with warnings denied.
7. Run broader workspace compile/tests proportionate to the diff.
8. Report changed files, migration guarantees, test evidence, and deferred
   follow-up Tasks.

## Stop condition

Stop this milestone only when all acceptance criteria are verified or a concrete
schema/data-loss risk requires a product decision. Do not continue into backup,
release signing, or installer work merely because this milestone completes.

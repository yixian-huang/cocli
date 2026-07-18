---
title: Workspace Provider and Portability Contract
category: architecture
tags: [workspace, provider, binding, attachment, backup, restore, migration]
updated: 2026-07-18
---

# Workspace Provider and Portability Contract

## Core decision

Separate three concepts that are currently combined:

- Workspace: portable logical identity and resource description.
- Subject attachment: relationship between a Workspace and an Agent or Channel.
- Machine binding: how one cocli installation resolves that Workspace locally.

The persisted descriptor is the alpha compatibility boundary. Internal Rust
traits and third-party plugin ABI remain unstable until the Beta extension
contract.

## Target storage model

### workspaces

- `id`
- `provider_key`
- `descriptor_version`
- `display_name`
- `portable_locator`
- `metadata_json`
- `created_at`
- `updated_at`

### subject_workspaces

- `workspace_id`
- `subject_type`: `agent` or `channel`
- `subject_id`
- `role`
- `attached_at`

### workspace_bindings

- `workspace_id`
- `installation_id`
- `local_locator`
- `state`
- `capabilities_json`
- `secret_ref`
- `last_verified_at`
- `error_code`
- `error_message`

The current installation identifier must be generated locally and excluded
from portable backups. Imported source-machine bindings may remain as hints but
must never be treated as current-machine bindings.

## Binding states

- `unbound`: no binding exists for this installation.
- `resolving`: validation or materialization is running.
- `ready`: the Provider resolved the resource successfully.
- `unavailable`: Provider implementation or required local dependency is absent.
- `needs_attention`: a candidate exists but validation failed or user input is
  required.

Agent/Channel state remains readable in every binding state.

## Internal Provider operations

- `describe`: human-readable description and capabilities.
- `validate`: validate descriptor and local binding without mutating external
  data.
- `bind`: create or replace a current-machine binding.
- `resolve`: return a usable path, URI, or opaque handle.
- `materialize`: optional explicit operation for managed or Git resources.
- `detach`: remove the subject relation or binding without deleting external
  data.

Providers do not mediate every filesystem write and do not own sandbox
approval, Diff review, Git commit policy, checkpoints, rollback, or validation
judgment.

## Built-in providers

### Managed

- Portable identity: cocli Workspace ID.
- Local binding: path below the cocli data directory.
- May be included in a backup bundle when the user requests managed data.

### Directory

- Portable identity: display name, purpose, and optional validation hints.
- Local binding: platform-specific absolute path.
- Rebinding is user-directed; cocli must not scan the entire disk automatically.

### Git

- Portable identity: canonical remote plus optional preferred ref.
- Local binding: existing checkout, existing worktree, or cocli-managed copy.
- Repository identity is not the current branch, HEAD, dirty state, or absolute
  path.
- Alpha requires reliable attachment and recognition of existing repositories
  and worktrees. Automatic clone/worktree creation can follow after this base.

### External

- Portable identity: provider key, URI, and external object identifier.
- Local binding: optional client capability and secret reference.
- Secrets are stored through OS facilities and excluded from portable data.

Unknown Provider descriptors must survive backup/restore and appear as
`unavailable`, not be discarded.

## API surface

- Read, update, and delete a Workspace.
- Attach/detach a Workspace to/from an Agent or Channel.
- List and update current-machine bindings.
- Verify or rebind a Workspace explicitly.
- Return structured state and error codes suitable for UI recovery.

Detaching must not delete a repository, directory, or external resource.
Cleanup of cocli-managed materializations is a separate, explicit, guarded
operation.

## Portable backup bundle

The preferred public-alpha backup is a versioned bundle containing:

- `manifest.json`
- `state.sqlite3`
- optional `managed-workspaces/`
- `checksums.json`

The manifest exposes enough information for preflight without mutating the
database: bundle format, app version, schema version, creation time, inventory
counts, required Provider keys, required Runtime keys, inclusions, and hashes.

Do not include OS credentials, active process state, reusable Bridge tokens, or
a promise that source-machine Runtime Sessions can resume.

## Restore and rebind sequence

1. Preflight bundle format, checksums, and schema compatibility.
2. Copy to staging and migrate the staged SQLite database.
3. Produce an inventory of Provider, Runtime, path, and credential gaps.
4. Atomically install state while preserving the current database.
5. Generate fresh local security tokens and reset live execution state.
6. Create current-installation Workspace and Runtime bindings.
7. Verify persistent subject counts and relational integrity.

Git may suggest a match by canonical remote. Directory rebinding requires a
user-selected path. Managed data can restore automatically when included.
Missing Runtimes keep Agent identity intact and mark execution unavailable.

## Alpha non-goals

- Merging two independently active cocli installations.
- Transparently resuming arbitrary source-machine Runtime Sessions.
- Copying external directories or Git repositories by default.
- Public third-party Workspace Provider ABI.
- Automatic destructive cleanup of repositories or worktrees.

## Acceptance criteria

1. A Workspace can be attached to multiple subjects without duplicating its
   logical identity.
2. All built-in Providers use the same state and error model.
3. Unknown Provider data round-trips without loss.
4. Moving a directory or repository produces a recoverable binding state.
5. Restoring between different data directories and filesystem roots preserves
   Agents, Channels, memberships, Tasks, messages, Memory, and Skills.
6. Missing Workspace, Runtime, or credentials do not make restored state
   unreadable.
7. Invalid restore input leaves the current installation untouched.

## Related pages

- [[cocli-self-bootstrap]]
- [[execution-goal-workspace-foundation]]
- [[public-alpha-distribution]]

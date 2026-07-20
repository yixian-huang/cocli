---
title: cocli Self-Bootstrap Program
category: architecture
tags: [self-bootstrap, agent, channel, workspace, portability, release]
updated: 2026-07-18
---

# cocli Self-Bootstrap Program

## Objective

Use cocli itself to organize, execute, observe, and verify the remaining public
alpha work. cocli is the durable coordination plane; Agent Runtimes perform the
actual editing, Git, build, test, and release operations.

Self-bootstrap does not mean that cocli replaces the operating system, an Agent
Runtime, Git, CI runners, Apple notarization, Windows code-signing services, or
the release host. These are explicit external execution and trust boundaries.

## Product invariants

- Agent and Channel are the two first-class persistent subjects.
- Work can start from an Agent or Channel without a Project, repository,
  directory, or Workspace.
- Workspace is an optional domain-neutral resource attachment.
- Session, Turn, PID, CLI, and process state remain diagnostic implementation
  details.
- Agents claim and organize durable Tasks; cocli is not a central intelligent
  scheduler.
- cocli does not own Agent reasoning, Diff review, checkpoint policy, rollback
  policy, validation judgment, or cross-Runtime budget enforcement.
- A missing Workspace, Runtime, credential, or local path degrades execution but
  does not destroy the persistent Agent, Channel, Task, Memory, or Skill state.

The canonical contract is `DESIGN.md`; `ROADMAP.md` tracks A4 Workspace
providers, A5 recovery and portability, and A6 installable public alpha.

## Current bootstrap baseline

- Durable Agents, Channels, memberships, direct Agent conversation, Tasks,
  Memory, Skills, self-organization Bridge operations, live events, search, and
  SQLite backup/restore exist.
- Workspace records currently combine subject ownership, adapter kind, locator,
  and metadata. Store and API behavior is primarily attach/list.
- Backup/restore safely stages and migrates SQLite, but external Workspace files
  and machine-specific bindings are outside the snapshot.
- CI already declares Linux x86_64/aarch64, macOS x86_64/arm64, and Windows
  x86_64 targets.
- Release workflow and installer scripts are placeholders.

## Bootstrap roles

- Workspace Agent: Workspace storage model, Provider boundary, Directory and
  Git implementations, API integration.
- Portability Agent: portable bundle, restore preflight, migration fixtures,
  cross-machine rebinding.
- Release Agent: artifact matrix, packaging, signing workflow, installers, and
  first-use path.
- Verification Agent: independently verifies acceptance criteria and can reject
  incomplete delivery evidence.

These are durable Agent responsibilities, not hard-coded runtime identities.
The same persistent Agent may change Runtime or model later.

## Program dependency graph

1. Workspace logical identity, subject attachment, and machine binding.
2. Internal Provider contract and common state model.
3. Directory and Git providers using existing resources.
4. Managed and external providers plus Workspace management UI.
5. Portable backup manifest and staged restore.
6. Rebinding assistant and cross-platform migration fixtures.
7. Release artifact matrix and clean-machine smoke tests.
8. OS signing, notarization, installers, and public alpha publication.

Release workflow scaffolding may proceed in parallel, but no public data or
compatibility promise should freeze before steps 1-3 establish the portable
Workspace descriptor.

## Bootstrap stages

### Stage 1: old capability builds the new boundary

Attach the cocli repository through the current Directory/Git locator behavior.
Use existing Agent, Channel, Task, Bridge, Runtime, and test capabilities to
implement the portable Workspace model.

### Stage 2: switch cocli development onto the new Provider model

Migrate the cocli repository attachment to one logical Workspace with one or
more subject attachments and a current-machine binding. Verify that path
changes and rebinding do not alter Agent or Channel identity.

### Stage 3: cocli migrates cocli

Export one cocli installation, restore it into a different data directory and
different filesystem location, rebind its Workspaces and Runtimes, and verify
the durable subjects and history.

### Stage 4: cocli coordinates its own release candidate

Agents prepare and verify artifacts through cocli. Protected CI environments
perform external signing, notarization, and publication.

## Required escape hatches

- Keep a known-good previous cocli binary.
- Preserve ordinary Git recovery independently of cocli.
- Keep offline backup/restore usable while the server is stopped.
- Preserve the pre-restore SQLite snapshot on failed or successful migration.
- Keep CI baseline tests runnable without an active cocli instance.
- Never make cocli the sole holder of signing or publication credentials.

## Completion criteria

Self-bootstrap is complete when:

1. Agents can create and claim cocli development Tasks inside cocli.
2. Multiple Agents can resolve the same logical cocli Workspace.
3. Moving the repository causes `needs_rebind`, not data loss.
4. A backup restores onto another supported platform with unresolved external
   resources clearly reported and repairable.
5. A Release Agent produces a candidate for every declared platform.
6. A separate Verification Agent validates runtime, backup/restore, installer,
   and signature evidence before publication.
7. External CI and signing services act only as execution/trust boundaries;
   cocli retains the durable organization and evidence trail.

## Related pages

- [[workspace-provider-portability]]
- [[public-alpha-distribution]]
- [[execution-goal-workspace-foundation]]

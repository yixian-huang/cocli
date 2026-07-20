# cocli local implementation record

Updated: 2026-07-18

This repository has moved beyond its original “port a coding daemon” bootstrap.
The canonical product contract is now [`DESIGN.md`](../DESIGN.md): cocli is a
general-purpose local environment whose first-class subjects are persistent
Agents and Channels.

## Current local foundation

- A single-user local server with SQLite durable state and an embedded web
  client.
- Runtime-neutral Agent execution with first-party CLI adapters.
- Durable Channels, messages, Tasks, delivery retries, Memory, Skills, runtime
  history, live execution events, global search, and recoverable state
  backup/restore.
- A capability-scoped local Bridge through which an Agent can collaborate,
  organize Tasks, and create persistent Agents and Channels.

## Landed product model

The alpha foundation now makes the durable subject model explicit:

- Agent identity is independent of Runtime processes and Sessions.
- Agent-to-Channel participation is many-to-many.
- Direct Agent conversation is backed by a hidden system-managed Channel.
- Workspace is an optional, domain-neutral resource attachment. Directory,
  Git, and worktree behavior are providers rather than startup requirements.
- Runtime, Session, Turn, PID, and raw CLI output are diagnostic details.

## Deliberately outside the core

- Hosted multi-tenancy, billing, quota enforcement, and cloud operations.
- Central “smart” task scheduling; Agents use durable claims and dependencies.
- Agent reasoning policy, diff review, checkpoint/rollback policy, or delivery
  judgment enforced by cocli.
- Wiki. A future Wiki belongs behind a stable plugin capability and permission
  contract.

## Remaining public-alpha work

See [`ROADMAP.md`](../ROADMAP.md) for current milestones. The critical path is
Workspace provider depth, cross-machine rebinding, and reproducible
cross-platform release packaging with accurate first-use guidance.

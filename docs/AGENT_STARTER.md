# Contributor starter context

Before changing cocli, read:

1. [`DESIGN.md`](../DESIGN.md)
2. [`README.md`](../README.md)
3. [`ROADMAP.md`](../ROADMAP.md)
4. [`CONTRIBUTING.md`](../CONTRIBUTING.md)

The invariant product model is:

- Agent and Channel are the two first-class starting subjects.
- Agent identity persists independently of Runtime, CLI, Session, and Turn.
- An Agent can join multiple Channels and can create persistent Agents,
  Channels, memberships, and Tasks through capability-scoped operations.
- Memory and Skills are Agent tools; execution history is diagnostics.
- Workspace is optional and domain-neutral. Project, directory, Git repository,
  and worktree behavior are providers, never startup requirements.
- Wiki is not a core module. It may return later only as a plugin over an
  explicit capability, permission, and storage contract.

Keep changes small and reversible. Verify targeted behavior first, then run the
workspace Rust checks, web tests/lint/build, and migration smoke tests before
claiming completion.

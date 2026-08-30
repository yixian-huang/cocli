# cocli Knowledge Base

Durable product and delivery knowledge for cocli. `DESIGN.md` and `ROADMAP.md`
are canonical; Wiki pages are supporting context and historical evidence.

## Current handoff

- Product center: persistent Agents, Channels, conversation, and membership.
- Current priority: A6 installable public alpha.
- Conversation-first Channel UX (Track C) and A5 recovery/portability through
  schema 19 are complete. See [[session-log-a5-schema19-recovery]].
- A4 Workspace-provider product work was descoped on 2026-07-20. Workspace
  remains optional infrastructure for Runtime cwd and portable rebinding.
- Portable Workspace foundation and backup/rebinding implementation are
  complete; do not restart those historical execution goals.

## Read first

- `DESIGN.md` — canonical product and interaction contract.
- `ROADMAP.md` — milestone status and product completion criteria.
- `README.md` — currently supported user-facing behavior.

## Supporting pages

- [[public-alpha-distribution]] — remaining A6 artifact, signing, installer,
  onboarding, and release gates.
- [[session-log-a5-schema19-recovery]] — A5 complete: schema 12 → 19, portable
  sanitization, and documented `cocli rebind`.
- [[session-log-portable-backup-rebinding]] — historical schema-12 A5
  implementation and verification evidence.
- [[cocli-self-bootstrap]] — historical program overview; partially superseded.
- [[workspace-provider-portability]] — historical implementation contract.
- [[execution-goal-workspace-foundation]] — completed historical goal.

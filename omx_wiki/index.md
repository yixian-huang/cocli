# cocli Knowledge Base

This directory is the durable knowledge base for cocli product and delivery work.
Pages use double-bracket wiki links so an Agent can discover the program from
this index without depending on an earlier chat session.

## Self-bootstrap program

- [[cocli-self-bootstrap]] — product boundary, bootstrap stages, task DAG, and
  completion criteria.
- [[workspace-provider-portability]] — Workspace identity, subject attachments,
  machine bindings, Provider behavior, backup bundles, and rebinding.
- [[public-alpha-distribution]] — artifact matrix, signing, installers, release
  gates, and first-use behavior.
- [[execution-goal-workspace-foundation]] — the first implementation goal and
  its acceptance criteria.

## Source-of-truth documents

- `DESIGN.md` — canonical product and interaction contract.
- `ROADMAP.md` — milestone status and product completion criteria.
- `README.md` — currently supported user-facing behavior.

## Reading order for a new execution session

1. Read `DESIGN.md` and `ROADMAP.md`.
2. Read [[cocli-self-bootstrap]].
3. Read [[workspace-provider-portability]].
4. Execute [[execution-goal-workspace-foundation]].
5. Use [[public-alpha-distribution]] only after the portable Workspace boundary
   is stable.

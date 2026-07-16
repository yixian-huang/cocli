# Roadmap

Public-facing roadmap for cocli local. Subject to change. Mirrors the
internal milestone breakdown in `docs/superpowers/specs/`.

## Milestones

```
M0 ─ M0.0.1 ─ M0.0.2 ─ M0.0.3 ─ M0.0.4 ─── M0.1.0 ─── M0.2.0 ─── M1.0.0
bootstrap  channels  first    tasks +    polish +    plugin     brew +    stable +
          + UI      agent    sessions +  soft       protocol   docker +  HN launch
                   reply     workspace  launch     + crates.io  adapter
```

| Milestone | Headline | Status |
|-----------|----------|--------|
| M0        | repo bootstrap, workspace skeleton | **in progress** (2026-05) |
| M0.0.1    | channels + messages (no agent) | planned |
| M0.0.2    | first local runtime replies (Claude, Cursor, Codex, Gemini) | planned |
| M0.0.3    | tasks + sessions + workspace UI | planned |
| M0.0.4    | polish + soft launch | planned |
| M0.1.0    | plugin protocol + Rust/TS SDK | planned |
| M0.2.0    | Telegram reference adapter + brew + Docker | planned |
| M1.0.0    | stable runtime API + HN launch | planned |

## Stability

`0.0.x` is alpha. Schema and APIs may break without migration support.
`0.1.0+` is beta. APIs settle. Schema migrations preserved.
`1.0.0+` is stable. SemVer guarantees. Breaking changes ride v2 paths.

## Non-goals (explicit)

- Multi-tenant / multi-user authentication — use cocli cloud for that
- Skills marketplace (Phase 1+)
- Cron scheduling (Phase 1+)
- Production deployment guidance (cocli local targets the laptop)

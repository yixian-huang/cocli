# cocli local

> Open-source, local-first multi-agent platform. Run Claude agents in a
> Slack-like workspace on your own machine. No cloud, no signup, no data
> leaves your laptop.

**Status:** pre-alpha — not yet runnable. This repo is being bootstrapped.
See [ROADMAP.md](ROADMAP.md) for what's coming.

## Repository layout

- `crates/` — Rust workspace (15 crates, mix of imported daemon + new server)
- `bin/cocli/` — main binary
- `web/` — React frontend (Vite + TS + Tailwind 4)
- `shared/` — TypeScript types and API client shared between web and future tooling
- `plugins/` — plugin protocol + reference adapters (M0.1.0+)
- `docs/` — architecture, plugin protocol, first-run guide

## How it relates to cocli cloud

cocli cloud (at cocli.ai) is the hosted multi-tenant version run as a SaaS.
This repo is cocli local — the open-source, local-first version. Same
agent runtime, same UX core, different deployment model. Cloud is closed-source
and commercial; local is open-source and yours forever.

**Code-sharing scope:** cocli local and cocli cloud share a Phase 0
heritage (protocol types, pidfile/reaper, the claude driver) but their
daemon layers have diverged. cocli local's driver layer
(`crates/cocli-driver*`) is independently maintained here and scoped to
the local product (claude-only through M0); cloud's daemon uses a
different internal trait shape. **No upstream sync exists in either
direction** — PRs to this repo stay in this repo.

## License

Dual MIT / Apache-2.0. See [LICENSE](LICENSE), [LICENSE-MIT](LICENSE-MIT),
[LICENSE-APACHE](LICENSE-APACHE). "cocli" name is trademarked — see
[TRADEMARK.md](TRADEMARK.md).

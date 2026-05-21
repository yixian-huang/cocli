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

## License

Dual MIT / Apache-2.0. See [LICENSE](LICENSE), [LICENSE-MIT](LICENSE-MIT),
[LICENSE-APACHE](LICENSE-APACHE). "cocli" name is trademarked — see
[TRADEMARK.md](TRADEMARK.md).

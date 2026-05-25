# Contributing to cocli

Thanks for your interest! Here's how to get up and running.

## Prerequisites

- Rust 1.78+ (`rustup install 1.78`)
- Node 20+ (for frontend dev)
- claude CLI installed (for end-to-end testing; not needed for unit tests)
- SQLite 3.38+ (sqlx bundles its own; only needed if you run sqlx-cli locally)

## Quick start

    git clone https://github.com/yixian/cocli && cd cocli
    cd web && npm ci && cd ..
    cargo build --workspace
    cargo test --workspace

The runtime stack is being assembled milestone-by-milestone — see
ROADMAP.md. As of M0, `cargo run --bin cocli` only prints a version
string. The full server bootstraps in M0.0.1.

## Tests

    cargo test --workspace                 # Rust unit tests
    cd web && npm test                     # Vitest
    # tests/e2e/ end-to-end suites land in later milestones

## Coding style

- `cargo fmt --all` must pass (CI enforces)
- `cargo clippy --workspace --all-targets -- -D warnings` must pass
- Frontend: `npm run lint` + `npx tsc --noEmit`

## DCO

We use the Developer Certificate of Origin. Sign your commits:

    git commit -s -m "your message"

A GitHub Action checks that every commit in a PR carries the
`Signed-off-by:` trailer.

## Pull requests

- Keep PRs focused. One milestone non-goal at a time.
- Reference the relevant spec section in the PR description.
- For changes touching > 1 crate, new public API surface, or schema
  migrations: open an RFC issue first (label `rfc:proposed`).

## Daemon / driver layer scope

`crates/cocli-driver` (the runtime trait) and `crates/cocli-driver-claude`
are the canonical driver-layer crates for cocli local. The closed-source
cocli cloud daemon uses a different internal trait shape (different crate
names, additional drivers). The two codebases share Phase 0 heritage but
**there is no sync mechanism** — patches to this repo are evaluated on
their merits here, not cherry-picked from cloud.

If you're considering a PR that adds a non-claude runtime driver (codex /
gemini / kimi / etc.), please open an `rfc:proposed` issue first — that's
out of M0 scope (claude-only) and needs scoping before code lands.

## Plugin authors

See `docs/plugin-protocol.md` (lands in M0.0.4). Adapters live in their
own repos or under `plugins/` if you want them shipped first-party.

## Reporting bugs / requesting features

Use the GitHub issue templates. For security issues, email
security@cocli.ai — do not file a public issue.

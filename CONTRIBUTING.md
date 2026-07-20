# Contributing to cocli

Thanks for your interest! Here's how to get up and running.

## Prerequisites

- Rust 1.78+ (`rustup install 1.78`)
- Node 20+ (for frontend dev)
- One supported Agent CLI for real Runtime end-to-end testing; not needed for
  unit tests or the fake-runtime local loop
- SQLite 3.38+ (sqlx bundles its own; only needed if you run sqlx-cli locally)

## Quick start

    git clone https://github.com/yixian/cocli && cd cocli
    cd web && npm ci && cd ..
    cargo build --workspace
    cargo test --workspace

Build the web client, then start the loopback-only local server:

    cd web && npm run build && cd ..
    cargo run --bin cocli -- --fake-runtime

Open `http://127.0.0.1:8090`. Omit `--fake-runtime` to discover installed
first-party Runtime CLIs.

## Tests

    cargo test --workspace                 # Rust unit tests
    cd web && npm test                     # Vitest
    npm run lint
    npm run build

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

- Keep PRs focused and preserve the product model in `DESIGN.md`.
- Reference the relevant `DESIGN.md` or `ROADMAP.md` section in the PR description.
- For changes touching > 1 crate, new public API surface, or schema
  migrations: open an RFC issue first (label `rfc:proposed`).

## Daemon / driver layer scope

The canonical shared driver layer lives in this repository. The first-party
production matrix is Claude, Cursor, Codex, Gemini, Kimi, Grok, Chatrs, and
OpenCode. Shared runtime fixes land here first with contract fixtures; cloud
consumes an exact OSS revision and does not retain parallel adapter,
discovery, bridge-injection, or driver-core implementations.

New runtime families beyond this production matrix require an `rfc:proposed`
issue. Changes to an existing adapter do not require an RFC when they preserve
the shared driver contract and include parser/spawn regression tests.

Before requesting review for runtime changes, run:

    cargo +1.78 test --workspace
    cargo +1.78 clippy --workspace --all-targets -- -D warnings
    cargo fmt --all -- --check
    scripts/check-runtime-release.sh

## Product boundaries

- Agent and Channel are persistent first-class subjects.
- Runtime, Session, Turn, and CLI process are implementation/diagnostic layers.
- Workspace is optional and domain-neutral; Git is one possible provider.
- Base Agent behavior must not assume software development.
- Wiki is reserved for a future plugin and must not be reintroduced into core
  navigation or Agent tools without an approved extension contract.

Plugin authoring documentation will land with the stable extension contract.

## Reporting bugs / requesting features

Use the GitHub issue templates. For security issues, email
security@cocli.ai — do not file a public issue.

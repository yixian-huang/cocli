# Runtime/Driver ownership and release policy

The `cocli` repository is the source of truth for reusable runtime code:

- `cocli-driver-core`
- `cocli-bridge-config`
- `cocli-runtime-pool`
- the Claude, Cursor, Codex, Gemini, Kimi, Grok, Chatrs, and OpenCode adapters
- the runtime-neutral agent contract and lifecycle integration

`cocli-cloud` is a consumer. It may configure binaries, register drivers,
translate the shared catalog into its wire protocol, and add SaaS lifecycle
behavior. It must not carry a second implementation of a driver, runtime model
discovery, bridge injection, or the shared driver contract.

## Versioning

The shared runtime crates start at `0.1.0`.

- Patch releases preserve public Rust types, driver behavior, event semantics,
  spawn/config formats, and discovery output.
- Minor releases may add optional trait methods, events, capabilities, models,
  or adapters. Existing consumers must continue to compile.
- Before `1.0.0`, a necessary breaking change increments the minor version and
  requires explicit migration notes.
- At and after `1.0.0`, normal SemVer rules apply.

Cloud production updates pin one exact OSS commit. A release tag may identify
that commit, but a branch or floating Git dependency is never a production
compatibility contract.

## Compatibility gate

Every runtime slice is complete only after:

1. Rust 1.78 workspace tests pass.
2. Workspace clippy passes with `-D warnings`.
3. Rustfmt and `git diff --check` pass.
4. Runtime crates pass the locked `cargo package --list` manifest/file gate;
   publish candidates additionally run `cargo package` in dependency order.
5. The target cloud revision compiles and tests against the exact OSS commit.
6. The cloud drift gate confirms there are no local runtime implementations or
   path dependencies.

The compatibility result records the OSS SHA, cloud SHA, public interfaces,
tests, and breaking changes in the project knowledge base.

## Change direction

Shared fixes land in OSS first. Cloud then upgrades its exact pin. Emergency
cloud fixes that touch shared behavior must be upstreamed before the cloud
branch is considered complete; copying the implementation back into cloud is
not an accepted long-term state.

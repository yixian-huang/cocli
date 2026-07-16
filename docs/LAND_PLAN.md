# Land plan: cocli local from cocli-cloud daemon-rs

Canonical long-form lives in Omni KB:

- `hub/cocli-local`
- `cocli-local/land-plan-from-daemon-rs`
- `cocli-local/agent-starter-prompt`

See also `docs/AGENT_STARTER.md` for the copy-paste agent prompt.

## One-liner

Port production-proven runtime pieces from `~/code/cocli-cloud/daemon-rs`
into `~/code/cocli` (OSS, SQLite, single-user), without multi-tenant cloud
server code, until `cargo run --bin cocli` can do channel + claude reply e2e.

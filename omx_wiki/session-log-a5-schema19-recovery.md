---
title: A5 Schema 19 Recovery Evidence
category: execution-log
tags: [recovery, portable, schema-19, rebind, completed]
updated: 2026-08-30
---

# A5 Schema 19 Recovery Evidence

Historical dogfood stopped at schema 12
([[session-log-portable-backup-rebinding]]). This page records the schema
12 → 19 upgrade, a schema-19 portable round-trip onto a second data dir,
sanitization that blocks apply replay, and the documented `cocli rebind`
CLI.

A5 is complete. Do not restart this work. A6 (installable public alpha)
is remaining.

## Implementation commits (Track R)

| Task | Commit | Subject |
|---|---|---|
| R1 | `022e1ab676329b97485b21585e2d8d73996dff68` | `test(store): restore schema 12 fixture through governance migrations` |
| R2 | `644e1c8a9705d5a5e7f778f0240683a6bd795d9d` | `test(cli): portable backup round-trip through schema 19 governance rows` |
| R3 | `0911bb1a580f12fa16828b60c14bde7fa374250d` | `test(api): portable restore cannot replay governance apply` |
| R4 | `7390f183ce2e9c201dba78475c3692c4d0d58911` | `feat(cli): add workspace rebind subcommand for portable restore` |

Evidence commands below were re-run on `7390f18` with
`CARGO_TARGET_DIR=/Users/yixian.huang/code/cocli/target`. Portable bundle
`state_sha256` values are generated per test run and are not recorded here.

## Schema 12 fixture → 19 via Store::open

Fixture: `crates/cocli-store/tests/fixtures/schema12_pre_governance.sql`

- SHA-256: `28a17bd30be87db3c6a8c54508331b6fe3de66c5ba6a3d5a599f87899e2d681d`
- Git blob at `022e1ab`: `598d524f2302bd788cc2e6ef2d24d4b3495cd4aa`

Tables from migrations 0001–0012 only. No 0013+ MCP/Skill governance
tables. Subject primary keys are 16-byte blobs (`sqlx` `Uuid`);
installation id stays TEXT.

| Subject | UUID |
|---|---|
| Channel | `aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa` |
| Agent | `bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb` |
| Message | `cccccccc-cccc-4ccc-8ccc-cccccccccccc` |
| Memory | `dddddddd-dddd-4ddd-8ddd-dddddddddddd` |
| Task | `eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee` |
| Workspace | `ffffffff-ffff-4fff-8fff-ffffffffffff` |
| Installation (TEXT) | `12121212-1212-4121-8121-121212121212` |

```
export CARGO_TARGET_DIR=/Users/yixian.huang/code/cocli/target
cargo test -p cocli-store portable_schema_upgrade -- --nocapture
```

```
schema12 fixture MAX(version) before Store::open = 12
schema12 fixture MAX(version) after Store::open = 19
test portable_schema_upgrade_restores_schema12_subjects_through_governance_migrations ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
```

`CURRENT_SCHEMA_VERSION` is 19. Channel, Agent, membership, user message,
Memory document, Task, and Workspace source binding survive `Store::open`
migrations 0013–0019.

## Schema 19 portable bundle (second data dir)

```
export CARGO_TARGET_DIR=/Users/yixian.huang/code/cocli/target
cargo test -p cocli --bin cocli -- portable --nocapture
```

```
running 5 tests
test portable::tests::staged_restore_bytes_must_match_the_preflight_checksum ... ok
test portable::tests::invalid_bundle_leaves_existing_installation_unchanged ... ok
test portable::tests::preflight_reports_providers_without_an_available_resolver ... ok
test portable::tests::bundle_round_trip_preserves_schema19_memory_and_governance_desired_state ... ok
test portable::tests::bundle_preflight_inventory_and_moved_git_rebind_round_trip ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 11.25s
```

`bundle_round_trip_preserves_schema19_memory_and_governance_desired_state`
runs `create_bundle` → `preflight_bundle` → `restore_bundle` into a fresh
data dir and asserts:

- `manifest.schema_version == 19` (`CURRENT_SCHEMA_VERSION`)
- restored installation id ≠ source
- Channel, Agent, message, Task, and Memory topic survive
- Skill profile + Workspace-scope binding and MCP profile + Agent-target
  binding survive
- current Workspace binding is `Unbound`; source-installation binding
  remains a hint
- `agent_bridge_tokens` count is 0
- no `running` agents (`AgentStatus::Stopped`)

Staged restore rechecks SHA-256 before installing
(`staged_restore_bytes_must_match_the_preflight_checksum`). Invalid
bundle bytes leave the live installation unchanged.

## Sanitization (apply cannot replay)

`sanitize_portable_state` still wipes installation identity (on restore),
Bridge tokens, working-state, running Agent status, open sessions,
in-flight deliveries, and binding `secret_ref`. R3 also drops:

- non-terminal MCP apply runs, then unreferenced `mcp_plan_decisions`
- non-terminal Skill apply runs and approved `skill_governance_plans`

Desired-state Skill/MCP profiles and bindings are unchanged. Hash-bound
apply of a pre-restore approval must fail closed.

Store sanitization source at `0911bb1`
(`crates/cocli-store/src/lib.rs` blob
`c5110d07a518d3d8295d37a19f9986e8644e8a6a`).

```
export CARGO_TARGET_DIR=/Users/yixian.huang/code/cocli/target
cargo test -p cocli-api --test governance_portable_restore -- --nocapture
cargo test -p cocli-api --test governance_integration portable_restore_cannot_replay_skill -- --nocapture
```

```
running 1 test
test portable_restore_cannot_replay_governance_apply ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.14s
```

```
running 1 test
test portable_restore_cannot_replay_skill_governance_apply ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.33s
```

Skill apply of a restored approval returned 200 OK before the
sanitization change; after the wipe it is rejected and `apply_mcp` is
not called for the MCP path.

## CLI rebind

```
cocli --data-dir <dir> rebind --workspace-id <uuid> --local-locator <path>
```

Wraps `Store::open` + `Store::bind_workspace`. Does not start the HTTP
server. Run only while the HTTP server is stopped. This binds a
Workspace **resource handle**, not a Git product workflow.

```
export CARGO_TARGET_DIR=/Users/yixian.huang/code/cocli/target
cargo test -p cocli --bin cocli -- rebind --nocapture
cargo run -p cocli --bin cocli --quiet -- rebind --help
```

```
running 3 tests
test tests::rebind_help_describes_resource_handle_not_git_product_workflow ... ok
test tests::rebind_parses_workspace_id_and_local_locator ... ok
test portable::tests::bundle_preflight_inventory_and_moved_git_rebind_round_trip ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 6 filtered out; finished in 0.22s
```

```
Bind a Workspace resource handle, not a Git product workflow.

Does not start the HTTP server. Run only while the HTTP server is stopped.

Usage: cocli rebind --workspace-id <WORKSPACE_ID> --local-locator <LOCAL_LOCATOR>
```

`bundle_preflight_inventory_and_moved_git_rebind_round_trip` restores
unbound, then `rebind_workspace` to a moved path with
`WorkspaceBindingState::Ready`.

## Status

Track R done: a schema-12 database upgrades to 19; a schema-19 portable
bundle restores on a second data dir; subjects and desired-state
governance survive; apply cannot replay; rebind is a documented CLI.

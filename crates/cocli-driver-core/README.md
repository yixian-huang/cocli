# cocli-driver-core

Runtime-neutral contract shared by cocli runtime adapters.

The crate contains:

- the object-safe `Driver` trait;
- optional process, stdin, interrupt, exit-code, and session-GC sub-traits;
- runtime-neutral events and lifecycle types;
- helpers for headless runtimes that exit after each turn.

It intentionally does not depend on cocli protocol, HTTP/WS connection,
SQLite/Postgres, tenant, billing, or other deployment-specific types.

## Consuming from cocli-cloud

Until crates.io releases begin, consumers must pin an exact cocli Git commit:

```toml
[dependencies]
cocli-driver-core = { git = "https://github.com/yixian/cocli", rev = "<exact-commit>" }
```

Do not depend on a branch. Upgrade through an explicit dependency change after
the compatibility gate passes:

```sh
COCLI_CLOUD_REPO=../cocli-cloud \
  scripts/check-driver-core-cloud-compat.sh
```

The compatibility script does not modify the cloud repository. It archives the
selected cloud revision into a temporary workspace, redirects every
`cocli-driver-core` dependency to this crate, and runs the daemon-rs workspace
tests.

After the OSS change has a commit, validate the exact dependency that cloud
will pin:

```sh
COCLI_CLOUD_REPO=../cocli-cloud \
COCLI_DRIVER_CORE_GIT_REV=<exact-commit> \
  scripts/check-driver-core-cloud-compat.sh
```

`COCLI_DRIVER_CORE_GIT_URL` can override the default GitHub repository.

## Upstream policy

After the first cloud cutover, this OSS crate is the single source of truth:

1. Change the contract and its tests in `cocli`.
2. Commit the OSS change and run the cloud compatibility gate against that
   exact revision.
3. Update only the pinned dependency and lockfile in `cocli-cloud`.

Do not maintain a second copy of the crate in cloud. An urgent cloud-side
contract fix must first be ported to this crate, then consumed by revision.

## Compatibility baseline

The initial public contract is ported from
`cocli-cloud/daemon-rs` commit `8d590a13`. The OSS crate is the intended
upstream after cloud switches to the pinned dependency.

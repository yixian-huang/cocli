#!/usr/bin/env bash
#
# Verify that cocli-cloud/daemon-rs can consume the OSS cocli-driver-core.
# The cloud repository is read-only: HEAD is archived into a temporary
# workspace, then every core dependency is redirected to the local crate.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
COCLI_REPO="$(cd "$SCRIPT_DIR/.." && pwd -P)"
COCLI_CORE="$(cd "$COCLI_REPO/crates/cocli-driver-core" && pwd -P)"
CLOUD_REPO="${COCLI_CLOUD_REPO:-$HOME/code/cocli-cloud}"
CLOUD_REV="${COCLI_CLOUD_REV:-HEAD}"
TOOLCHAIN="${COCLI_CLOUD_TOOLCHAIN:-1.85}"
CORE_GIT_REV="${COCLI_DRIVER_CORE_GIT_REV:-}"
CORE_GIT_URL="${COCLI_DRIVER_CORE_GIT_URL:-https://github.com/yixian/cocli}"

if [[ ! -d "$CLOUD_REPO/.git" ]]; then
    echo "cocli-cloud repository not found: $CLOUD_REPO" >&2
    exit 2
fi

WORK_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/cocli-driver-core-cloud-compat.XXXXXX")"
SHADOW="$WORK_ROOT/daemon-rs"

if [[ -n "$CORE_GIT_REV" ]]; then
    CORE_DEP_SPEC="git = \"$CORE_GIT_URL\", rev = \"$CORE_GIT_REV\""
    CORE_SOURCE="$CORE_GIT_URL@$CORE_GIT_REV"
else
    CORE_DEP_SPEC="path = \"$COCLI_CORE\""
    CORE_SOURCE="$COCLI_CORE"
fi

cleanup() {
    if [[ "${KEEP_COCLI_DRIVER_CORE_COMPAT_WORKDIR:-0}" == "1" ]]; then
        echo "kept compatibility workspace: $WORK_ROOT"
        return
    fi
    find "$WORK_ROOT" -depth -delete
}
trap cleanup EXIT

git -C "$CLOUD_REPO" archive "$CLOUD_REV" \
    daemon-rs \
    internal/protocol/daemon_msg.go |
    tar -x -C "$WORK_ROOT"

while IFS= read -r -d '' manifest; do
    CORE_DEP_SPEC="$CORE_DEP_SPEC" perl -0pi -e '
        s{
            cocli-driver-core \s* = \s*
            \{ \s* path \s* = \s* "[^"]+" \s* \}
        }{
            qq{cocli-driver-core = { $ENV{CORE_DEP_SPEC} }}
        }gex
    ' "$manifest"
done < <(find "$SHADOW" -name Cargo.toml -type f -print0)

perl -0pi -e 's{\n\s*"crates/cocli-driver-core",}{}' "$SHADOW/Cargo.toml"

if grep -R --include Cargo.toml -n \
    'cocli-driver-core[[:space:]]*=[[:space:]]*{[[:space:]]*path[[:space:]]*=[[:space:]]*"\.\.' \
    "$SHADOW"
then
    echo "not every cocli-driver-core dependency was redirected" >&2
    exit 1
fi

echo "cloud revision: $(git -C "$CLOUD_REPO" rev-parse "$CLOUD_REV")"
echo "OSS core: $CORE_SOURCE"
echo "shadow workspace: $SHADOW"

CARGO_TARGET_DIR="$WORK_ROOT/target" \
    cargo +"$TOOLCHAIN" test --manifest-path "$SHADOW/Cargo.toml" --workspace

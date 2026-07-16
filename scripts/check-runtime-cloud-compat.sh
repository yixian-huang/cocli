#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
COCLI_REPO="$(cd "$SCRIPT_DIR/.." && pwd -P)"
CLOUD_REPO="${COCLI_CLOUD_REPO:-$HOME/code/cocli-cloud}"
CLOUD_REV="${COCLI_CLOUD_REV:-HEAD}"
TOOLCHAIN="${COCLI_CLOUD_TOOLCHAIN:-1.85}"
HOST_CARGO_BIN="$(dirname "$(command -v cargo)")"
TEST_PATH="${COCLI_CLOUD_TEST_PATH:-$HOST_CARGO_BIN:/usr/bin:/bin:/usr/sbin:/sbin}"

runtime_crates=(
  cocli-driver-core
  cocli-bridge-config
  cocli-driver-chatrs
  cocli-driver-claude
  cocli-driver-codex
  cocli-driver-cursor
  cocli-driver-gemini
  cocli-driver-grok
  cocli-driver-kimi
  cocli-driver-opencode
  cocli-runtime-pool
)

if ! git -C "$CLOUD_REPO" rev-parse --git-dir >/dev/null 2>&1; then
  echo "cocli-cloud repository not found: $CLOUD_REPO" >&2
  exit 2
fi

WORK_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/cocli-runtime-cloud-compat.XXXXXX")"
SHADOW="$WORK_ROOT/daemon-rs"

cleanup() {
  if [[ "${KEEP_COCLI_RUNTIME_COMPAT_WORKDIR:-0}" == "1" ]]; then
    echo "kept compatibility workspace: $WORK_ROOT"
    return
  fi
  find "$WORK_ROOT" -depth -delete
}
trap cleanup EXIT

git -C "$CLOUD_REPO" archive "$CLOUD_REV" daemon-rs internal/protocol/daemon_msg.go |
  tar -x -C "$WORK_ROOT"

if find "$SHADOW/crates" -maxdepth 1 -type d \
  \( -name 'cocli-driver-*' -o -name 'cocli-runtime-pool' -o -name 'cocli-bridge-config' \) |
  grep -q .
then
  echo "cloud revision still contains shared runtime implementation directories" >&2
  exit 1
fi

for crate in "${runtime_crates[@]}"; do
  crate_path="$COCLI_REPO/crates/$crate"
  if [[ ! -d "$crate_path" ]]; then
    echo "missing OSS runtime crate: $crate_path" >&2
    exit 1
  fi
  CRATE="$crate" CRATE_PATH="$crate_path" perl -0pi -e '
    s{
      \Q$ENV{CRATE}\E \s* = \s*
      \{ [^}\n]* \}
    }{
      qq{$ENV{CRATE} = { path = "$ENV{CRATE_PATH}" }}
    }gex
  ' "$SHADOW/Cargo.toml"
done

for crate in "${runtime_crates[@]}"; do
  if ! grep -Fq "$crate = { path = \"$COCLI_REPO/crates/$crate\" }" "$SHADOW/Cargo.toml"; then
    echo "cloud dependency was not redirected: $crate" >&2
    exit 1
  fi
done

echo "cloud revision: $(git -C "$CLOUD_REPO" rev-parse "$CLOUD_REV")"
echo "OSS runtime source: $COCLI_REPO"
echo "shadow workspace: $SHADOW"

CARGO_TARGET_DIR="$WORK_ROOT/target" \
  PATH="$TEST_PATH" \
  cargo +"$TOOLCHAIN" test --manifest-path "$SHADOW/Cargo.toml" --workspace

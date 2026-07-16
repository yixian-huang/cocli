#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$ROOT"

crates=(
  cocli-driver-core
  cocli-bridge-config
  cocli-driver-claude
  cocli-driver-cursor
  cocli-driver-codex
  cocli-driver-gemini
  cocli-driver-kimi
  cocli-driver-grok
  cocli-driver-chatrs
  cocli-driver-opencode
  cocli-runtime-pool
)

for crate in "${crates[@]}"; do
  version="$(
    cargo metadata --no-deps --format-version 1 |
      jq -r --arg crate "$crate" '.packages[] | select(.name == $crate) | .version'
  )"
  if [[ "$version" != "0.1.0" ]]; then
    echo "$crate must be version 0.1.0, found $version" >&2
    exit 1
  fi
  cargo package --locked --allow-dirty --list -p "$crate" >/dev/null
done

echo "runtime release gate: PASS"

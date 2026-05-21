#!/usr/bin/env bash
# sync-daemon-rs.sh — rsync the 8 reused daemon crates between cocli and 1HzAi.
#
# Usage:
#   scripts/sync-daemon-rs.sh pull           # 1HzAi/daemon-rs → cocli/crates
#   scripts/sync-daemon-rs.sh push           # cocli/crates → 1HzAi/daemon-rs
#   scripts/sync-daemon-rs.sh diff           # show what would change in pull direction
#
# Env overrides:
#   COCLI_REPO        default: $HOME/code/cocli
#   ONEHZAI_REPO      default: $HOME/code/1HzAi
#
# IMPORTANT: This script does NOT touch Cargo.toml [workspace*] blocks.
# When pulling from 1HzAi, the standalone [workspace] tables (parallel-dev
# isolation, per daemon-rs/MORNING_VERIFICATION.md §6) must be stripped
# manually after sync. See M0 plan Task 2.x for the procedure.

set -euo pipefail

CRATES=(
    cocli-protocol
    cocli-actor
    cocli-pidfile
    cocli-reaper
    cocli-driver-claude
    cocli-bridge-config
    cocli-agent
    cocli-health
)

COCLI_REPO="${COCLI_REPO:-$HOME/code/cocli}"
ONEHZAI_REPO="${ONEHZAI_REPO:-$HOME/code/1HzAi}"
ACTION="${1:-}"

if [[ -z "$ACTION" ]]; then
    grep -E '^#' "$0" | head -20 | sed 's/^# \{0,1\}//'
    exit 2
fi

cd_or_die() {
    [[ -d "$1" ]] || { echo "missing dir: $1" >&2; exit 1; }
}
cd_or_die "$COCLI_REPO"
cd_or_die "$ONEHZAI_REPO/daemon-rs"

case "$ACTION" in
    pull)
        for c in "${CRATES[@]}"; do
            src="$ONEHZAI_REPO/daemon-rs/crates/$c/"
            dst="$COCLI_REPO/crates/$c/"
            [[ -d "$src" ]] || { echo "source missing: $src" >&2; exit 1; }
            echo "→ pulling $c"
            rsync -a --delete --exclude=target \
                "$src" "$dst"
        done
        echo
        echo "Pull complete. Run 'cargo build --workspace' to verify."
        echo "If [workspace*] blocks reappeared in any Cargo.toml, strip them per M0 §2."
        ;;
    push)
        for c in "${CRATES[@]}"; do
            src="$COCLI_REPO/crates/$c/"
            dst="$ONEHZAI_REPO/daemon-rs/crates/$c/"
            [[ -d "$src" ]] || { echo "source missing: $src" >&2; exit 1; }
            echo "→ pushing $c"
            rsync -a --delete --exclude=target \
                --exclude=Cargo.toml \
                "$src" "$dst"
        done
        echo
        echo "Push complete. NOTE: Cargo.toml excluded because the workspace inheritance"
        echo "shape differs between cocli (root-level workspace) and daemon-rs (standalone"
        echo "[workspace] blocks). Apply Cargo.toml changes manually after review."
        ;;
    diff)
        for c in "${CRATES[@]}"; do
            src="$ONEHZAI_REPO/daemon-rs/crates/$c/"
            dst="$COCLI_REPO/crates/$c/"
            echo "── $c ──"
            diff -rq "$src" "$dst" 2>/dev/null | grep -v '^Only in.*: target' || true
        done
        ;;
    *)
        echo "unknown action: $ACTION (use: pull / push / diff)" >&2
        exit 2
        ;;
esac

#!/usr/bin/env bash
# Clean-prefix fake-runtime smoke for an installed cocli (+ cocli-bridge).
#
# Starts with --fake-runtime --data-dir <tmp>, curls /api/doctor and
# /api/runtimes, then SIGTERM. Does not touch the default data directory.
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: smoke-installed.sh

Environment:
  COCLI_BIN          Path to installed cocli (default: cocli on PATH)
  COCLI_BRIDGE_BIN   Path to installed cocli-bridge (default: beside cocli)
  COCLI_SMOKE_PORT   Loopback port (default: 18091)
  COCLI_SMOKE_KEEP   If 1, keep the temp data dir on exit
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi
if [[ $# -gt 0 ]]; then
  echo "smoke-installed.sh: unexpected argument: $*" >&2
  usage >&2
  exit 2
fi

die() {
  echo "smoke-installed.sh: $*" >&2
  exit 1
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

need_cmd curl
need_cmd python3
CURL=(curl -sf --connect-timeout 2 --max-time 30)

resolve_bin() {
  local given=$1
  if [[ -n "$given" ]]; then
    [[ -x "$given" ]] || die "not executable: $given"
    echo "$given"
    return 0
  fi
  command -v cocli 2>/dev/null || die "cocli not found; set COCLI_BIN or install onto PATH"
}

BIN=$(resolve_bin "${COCLI_BIN:-}")
BIN_DIR=$(cd "$(dirname "$BIN")" && pwd -P)
BIN="$BIN_DIR/$(basename "$BIN")"
BRIDGE=${COCLI_BRIDGE_BIN:-"$BIN_DIR/cocli-bridge"}
if [[ ! -x "$BRIDGE" ]]; then
  die "cocli-bridge not found beside cocli (looked at $BRIDGE); set COCLI_BRIDGE_BIN"
fi

echo "cocli:        $BIN"
echo "cocli-bridge: $BRIDGE"
"$BIN" --version
"$BRIDGE" --version

PORT=${COCLI_SMOKE_PORT:-18091}
BASE="http://127.0.0.1:${PORT}"
DATA=$(mktemp -d "${TMPDIR:-/tmp}/cocli-installed-smoke.XXXXXX")
PID=""

cleanup() {
  if [[ -n "$PID" ]] && kill -0 "$PID" 2>/dev/null; then
    kill -TERM "$PID" 2>/dev/null || true
    for _ in $(seq 1 50); do
      if ! kill -0 "$PID" 2>/dev/null; then
        break
      fi
      sleep 0.1
    done
    if kill -0 "$PID" 2>/dev/null; then
      echo "smoke-installed.sh: SIGTERM did not stop pid $PID; sending SIGKILL" >&2
      kill -KILL "$PID" 2>/dev/null || true
    fi
    wait "$PID" 2>/dev/null || true
  fi
  if [[ "${COCLI_SMOKE_KEEP:-0}" == "1" ]]; then
    echo "kept data dir: $DATA"
  else
    rm -rf "$DATA"
  fi
}
trap cleanup EXIT

echo "data dir: $DATA"
echo "bind:     $BASE"

"$BIN" --fake-runtime --data-dir "$DATA" --bind "127.0.0.1:${PORT}" >"$DATA/server.log" 2>&1 &
PID=$!

ready=0
for _ in $(seq 1 100); do
  if "${CURL[@]}" "$BASE/healthz" >/dev/null 2>&1; then
    ready=1
    break
  fi
  if ! kill -0 "$PID" 2>/dev/null; then
    wait "$PID" 2>/dev/null || true
    echo "server log:" >&2
    cat "$DATA/server.log" >&2 || true
    die "cocli exited before becoming ready"
  fi
  sleep 0.1
done
[[ "$ready" -eq 1 ]] || {
  echo "server log:" >&2
  cat "$DATA/server.log" >&2 || true
  die "timeout waiting for $BASE/healthz"
}

echo "— /healthz —"
"${CURL[@]}" "$BASE/healthz"
echo

echo "— /api/runtimes —"
RUNTIMES=$("${CURL[@]}" "$BASE/api/runtimes")
echo "$RUNTIMES" | python3 -c 'import json,sys
data=json.load(sys.stdin)
assert isinstance(data, list) and data, "expected a non-empty /api/runtimes list"
fake=[r for r in data if r.get("name")=="fake"]
assert fake and fake[0].get("installed") is True, "fake runtime missing or not installed: %r" % (data,)
print(json.dumps(fake[0], indent=2))'

echo "— /api/doctor —"
DOCTOR=$("${CURL[@]}" "$BASE/api/doctor")
echo "$DOCTOR" | python3 -c 'import json,sys
data=json.load(sys.stdin)
assert data.get("schemaVersion"), "missing schemaVersion on /api/doctor"
assert "summary" in data and "runtimes" in data, "doctor report missing summary/runtimes"
print(json.dumps({"schemaVersion": data.get("schemaVersion"), "summary": data.get("summary")}, indent=2))'

echo "smoke-installed: PASS"

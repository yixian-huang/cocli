#!/usr/bin/env bash
# Restore the R1 schema-12 SQL fixture with a release cocli binary.
#
# Loads crates/cocli-store/tests/fixtures/schema12_pre_governance.sql into a
# SQLite snapshot (rewriting unhex() to blob literals), then:
#   COCLI_DATA_DIR=<tmp> "$COCLI_BIN" restore --input <snapshot>
# Asserts MAX(version) == 19. Does not start the HTTP server.
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: restore-release-fixture.sh

Environment:
  COCLI_BIN          Path to the release cocli binary (required)
  COCLI_DATA_DIR     Restore destination (default: a temp dir, deleted on exit)
  COCLI_FIXTURE_SQL  Override schema-12 SQL path
  COCLI_RESTORE_KEEP If 1, keep the temp data dir on exit
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi
if [[ $# -gt 0 ]]; then
  echo "restore-release-fixture: unexpected argument: $*" >&2
  usage >&2
  exit 2
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"

die() {
  echo "restore-release-fixture: $*" >&2
  exit 1
}

python_bin=""
if command -v python3 >/dev/null 2>&1; then
  python_bin=python3
elif command -v python >/dev/null 2>&1; then
  python_bin=python
else
  die "python3 or python is required"
fi
[[ -n "${COCLI_BIN:-}" ]] || die "set COCLI_BIN to the release cocli binary"
[[ -f "$COCLI_BIN" ]] || die "not found: $COCLI_BIN"
if [[ ! -x "$COCLI_BIN" ]]; then
  chmod +x "$COCLI_BIN" || die "not executable: $COCLI_BIN"
fi

BIN_DIR=$(cd "$(dirname "$COCLI_BIN")" && pwd -P)
BIN="$BIN_DIR/$(basename "$COCLI_BIN")"

FIXTURE=${COCLI_FIXTURE_SQL:-"$ROOT/crates/cocli-store/tests/fixtures/schema12_pre_governance.sql"}
[[ -f "$FIXTURE" ]] || die "schema-12 fixture not found: $FIXTURE"

KEEP=${COCLI_RESTORE_KEEP:-0}
OWNED_DATA=0
if [[ -n "${COCLI_DATA_DIR:-}" ]]; then
  DATA=$COCLI_DATA_DIR
  mkdir -p "$DATA"
else
  DATA=$(mktemp -d "${TMPDIR:-/tmp}/cocli-release-restore.XXXXXX")
  OWNED_DATA=1
fi
WORK=$(mktemp -d "${TMPDIR:-/tmp}/cocli-release-restore-work.XXXXXX")

cleanup() {
  rm -rf "$WORK"
  if [[ "$OWNED_DATA" -eq 1 && "$KEEP" != "1" ]]; then
    rm -rf "$DATA"
  elif [[ "$KEEP" == "1" ]]; then
    echo "kept data dir: $DATA"
  fi
}
trap cleanup EXIT

SNAPSHOT="$WORK/schema12_pre_governance.sqlite3"

echo "cocli:   $BIN"
"$BIN" --version
echo "fixture: $FIXTURE"
echo "data:    $DATA"

"$python_bin" - "$FIXTURE" "$SNAPSHOT" <<'PY'
import re
import sqlite3
import sys
from pathlib import Path

sql_path = Path(sys.argv[1])
db_path = Path(sys.argv[2])
sql = sql_path.read_text()


def repl(match):
    hex_id = match.group(1).replace("-", "")
    if len(hex_id) != 32:
        raise SystemExit("unexpected uuid hex length: %s" % match.group(1))
    return "X'%s'" % hex_id


sql, count = re.subn(
    r"unhex\(\s*replace\(\s*'([0-9a-fA-F-]+)'\s*,\s*'-'\s*,\s*''\s*\)\s*\)",
    repl,
    sql,
)
if count == 0:
    raise SystemExit("fixture contained no unhex(replace(...)) uuid blobs")

if db_path.exists():
    db_path.unlink()
con = sqlite3.connect(db_path)
try:
    con.executescript(sql)
    row = con.execute("SELECT MAX(version) FROM cocli_schema_migrations").fetchone()
    if not row or row[0] != 12:
        raise SystemExit(f"loaded fixture schema is {row}, expected 12")
    channels = con.execute("SELECT COUNT(*) FROM channels").fetchone()[0]
    if channels < 1:
        raise SystemExit("loaded fixture has no channels")
finally:
    con.close()
print(f"wrote snapshot {db_path} (schema 12, {count} uuid blobs)")
PY

echo "— cocli restore —"
COCLI_DATA_DIR="$DATA" "$BIN" restore --input "$SNAPSHOT"

"$python_bin" - "$DATA" <<'PY'
import sqlite3
import sys
from pathlib import Path

data = Path(sys.argv[1])
db = data / "cocli.sqlite3"
if not db.is_file():
    raise SystemExit(f"missing restored database: {db}")
con = sqlite3.connect(db)
try:
    version = con.execute("SELECT MAX(version) FROM cocli_schema_migrations").fetchone()[0]
    channels = con.execute("SELECT COUNT(*) FROM channels").fetchone()[0]
    agents = con.execute("SELECT COUNT(*) FROM agents").fetchone()[0]
finally:
    con.close()
print(f"restored schema MAX(version)={version} channels={channels} agents={agents}")
if version != 19:
    raise SystemExit(f"expected schema 19 after restore, got {version}")
if channels < 1 or agents < 1:
    raise SystemExit("restored database missing schema-12 subjects")
PY

echo "restore-release-fixture: PASS"

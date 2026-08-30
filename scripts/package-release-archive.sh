#!/usr/bin/env bash
# Stage cocli + cocli-bridge, licenses, INSTALL.txt, inner SHA256SUMS, then
# write cocli-${VERSION}-${TARGET}.tar.gz (unix) or .zip (windows).
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: package-release-archive.sh <version> <target> <bin_dir> <out_dir>

<bin_dir> must contain cocli and cocli-bridge (or .exe on windows targets).
Writes the archive (and only the archive) into <out_dir>. Inner SHA256SUMS
lists cocli / cocli-bridge names for optional installer verification after
extract.
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi
if [[ $# -ne 4 ]]; then
  usage >&2
  exit 2
fi

VERSION=$1
TARGET=$2
BIN_DIR=$3
OUT_DIR=$4

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"

die() {
  echo "package-release-archive: $*" >&2
  exit 1
}

[[ -n "$VERSION" ]] || die "empty version"
[[ -n "$TARGET" ]] || die "empty target"
[[ -d "$BIN_DIR" ]] || die "bin dir is not a directory: $BIN_DIR"
mkdir -p "$OUT_DIR"
OUT_DIR=$(cd "$OUT_DIR" && pwd -P)
BIN_DIR=$(cd "$BIN_DIR" && pwd -P)

case "$TARGET" in
  *-pc-windows-msvc)
    NAMES="cocli.exe cocli-bridge.exe"
    ARCHIVE="cocli-${VERSION}-${TARGET}.zip"
    ;;
  *)
    NAMES="cocli cocli-bridge"
    ARCHIVE="cocli-${VERSION}-${TARGET}.tar.gz"
    ;;
esac

STAGE=$(mktemp -d "${TMPDIR:-/tmp}/cocli-release-stage.XXXXXX")
cleanup() {
  rm -rf "$STAGE"
}
trap cleanup EXIT

# shellcheck disable=SC2086
for name in $NAMES; do
  [[ -f "$BIN_DIR/$name" ]] || die "missing binary $BIN_DIR/$name"
  cp "$BIN_DIR/$name" "$STAGE/$name"
  chmod 755 "$STAGE/$name"
done

for license in LICENSE LICENSE-MIT LICENSE-APACHE; do
  [[ -f "$ROOT/$license" ]] || die "missing $license"
  cp "$ROOT/$license" "$STAGE/$license"
done

cat >"$STAGE/INSTALL.txt" <<EOF
cocli ${VERSION} (${TARGET})

This archive contains:
  ${NAMES}
  LICENSE, LICENSE-MIT, LICENSE-APACHE (MIT OR Apache-2.0)
  SHA256SUMS (inner binaries)
  INSTALL.txt

Install (user-scoped, no admin):
  Unix:    COCLI_VERSION=${VERSION} ./scripts/install.sh
  Windows: \$env:COCLI_VERSION="${VERSION}"; .\\scripts\\install.ps1

Or copy both binaries onto PATH. Verify the archive against SHA256SUMS on
the GitHub Release before replacing anything. The installer never touches
the data directory.

Unsigned unless the release notes say Apple/Windows signing secrets were
present. Missing secrets print "unsigned draft".

Next: run \`cocli\`, then open http://127.0.0.1:8090
EOF

# Inner names (cocli / cocli-bridge or .exe) — optional extra check after extract.
# Release-level SHA256SUMS hashes archives only (inner names collide across targets).
# shellcheck disable=SC2086
"$ROOT/scripts/write-sha256sums.sh" "$STAGE" $NAMES >/dev/null

CONTENTS="LICENSE LICENSE-MIT LICENSE-APACHE INSTALL.txt SHA256SUMS"
# shellcheck disable=SC2086
for name in $NAMES; do
  CONTENTS="$CONTENTS $name"
done

DEST="$OUT_DIR/$ARCHIVE"
rm -f "$DEST"

case "$ARCHIVE" in
  *.zip)
    python_bin=""
    if command -v python3 >/dev/null 2>&1; then
      python_bin=python3
    elif command -v python >/dev/null 2>&1; then
      python_bin=python
    else
      die "python3 or python is required to write zip archives"
    fi
    # shellcheck disable=SC2086
    "$python_bin" - "$STAGE" "$DEST" $CONTENTS <<'PY'
import sys
import zipfile
from pathlib import Path

stage = Path(sys.argv[1])
dest = Path(sys.argv[2])
names = sys.argv[3:]
with zipfile.ZipFile(dest, "w", compression=zipfile.ZIP_DEFLATED) as zf:
    for name in names:
        path = stage / name
        if not path.is_file():
            raise SystemExit(f"missing staged file: {name}")
        zf.write(path, name)
PY
    ;;
  *)
    # Avoid macOS AppleDouble entries in the tarball.
    # shellcheck disable=SC2086
    COPYFILE_DISABLE=1 tar -czf "$DEST" -C "$STAGE" $CONTENTS
    ;;
esac

[[ -f "$DEST" ]] || die "failed to write $DEST"
echo "wrote $DEST"
ls -l "$DEST"

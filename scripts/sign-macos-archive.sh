#!/usr/bin/env bash
# codesign cocli + cocli-bridge inside a darwin .tar.gz and rewrite the archive.
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: sign-macos-archive.sh <version> <archive> <out_dir>

Requires:
  APPLE_CERTIFICATE_P12        base64-encoded Developer ID .p12
  APPLE_CERTIFICATE_PASSWORD   PKCS#12 password
  APPLE_SIGNING_IDENTITY       optional; default Developer ID Application
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi
if [[ $# -ne 3 ]]; then
  usage >&2
  exit 2
fi

VERSION=$1
ARCHIVE=$2
OUT_DIR=$3

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"

die() {
  echo "sign-macos-archive: $*" >&2
  exit 1
}

[[ -f "$ARCHIVE" ]] || die "archive not found: $ARCHIVE"
[[ -n "${APPLE_CERTIFICATE_P12:-}" ]] || die "APPLE_CERTIFICATE_P12 is empty"
[[ -n "${APPLE_CERTIFICATE_PASSWORD:-}" ]] || die "APPLE_CERTIFICATE_PASSWORD is empty"

IDENTITY=${APPLE_SIGNING_IDENTITY:-Developer ID Application}
ARCHIVE=$(cd "$(dirname "$ARCHIVE")" && pwd -P)/$(basename "$ARCHIVE")
mkdir -p "$OUT_DIR"
OUT_DIR=$(cd "$OUT_DIR" && pwd -P)

base=$(basename "$ARCHIVE")
case "$base" in
  cocli-"$VERSION"-*-apple-darwin.tar.gz) ;;
  *) die "expected darwin tar.gz named cocli-${VERSION}-<triple>-apple-darwin.tar.gz, got $base" ;;
esac
prefix="cocli-${VERSION}-"
target=${base#"$prefix"}
target=${target%.tar.gz}

WORK=$(mktemp -d "${TMPDIR:-/tmp}/cocli-sign-macos.XXXXXX")
KEYCHAIN="$WORK/cocli-release.keychain-db"
P12="$WORK/cert.p12"
BINS="$WORK/bins"
cleanup() {
  security delete-keychain "$KEYCHAIN" >/dev/null 2>&1 || true
  rm -rf "$WORK"
}
trap cleanup EXIT

mkdir -p "$BINS"
tar -xzf "$ARCHIVE" -C "$WORK"
[[ -f "$WORK/cocli" && -f "$WORK/cocli-bridge" ]] || die "archive is missing cocli / cocli-bridge"
cp "$WORK/cocli" "$WORK/cocli-bridge" "$BINS/"

# Do not log certificate material.
echo "$APPLE_CERTIFICATE_P12" | base64 --decode >"$P12"
KEYCHAIN_PASSWORD=$(uuidgen)

security create-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
security set-keychain-settings -lut 21600 "$KEYCHAIN"
security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
security import "$P12" -k "$KEYCHAIN" -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign -T /usr/bin/security
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$KEYCHAIN_PASSWORD" "$KEYCHAIN" >/dev/null
# shellcheck disable=SC2046
security list-keychains -d user -s "$KEYCHAIN" $(security list-keychains -d user | sed 's/"//g')

codesign --force --options runtime --timestamp --sign "$IDENTITY" "$BINS/cocli" "$BINS/cocli-bridge"
codesign --verify --verbose=2 "$BINS/cocli" "$BINS/cocli-bridge"

"$ROOT/scripts/package-release-archive.sh" "$VERSION" "$target" "$BINS" "$OUT_DIR"
echo "signed $base"

#!/usr/bin/env bash
# User-scoped installer for cocli + cocli-bridge (unsigned checksum path).
#
# Selects a version (COCLI_VERSION or the latest GitHub release) and OS/arch,
# verifies SHA-256 before replacing, installs both binaries together, and
# never touches the data directory.
#
# Until GitHub Releases exist, set COCLI_ARTIFACT_DIR to a directory of locally
# built binaries plus SHA256SUMS (see scripts/write-sha256sums.sh).
#
# Release asset names (D3 must match):
#   cocli-${VERSION}-${TARGET}.tar.gz   # unix archive: both binaries
#   cocli-${VERSION}-${TARGET}.zip      # windows archive: both binaries
#   SHA256SUMS
set -euo pipefail

REPO_DEFAULT="yixian-huang/cocli"
UI_URL="http://127.0.0.1:8090"

usage() {
  cat <<'EOF'
Usage: install.sh

Install cocli and cocli-bridge into a user-scoped prefix (no admin).

Environment:
  COCLI_ARTIFACT_DIR  Directory of local binaries + SHA256SUMS (skips GitHub)
  COCLI_VERSION       Release version (default: latest GitHub release)
  COCLI_PREFIX        Install prefix (default: $HOME/.local)
  COCLI_REPO          GitHub owner/repo (default: yixian-huang/cocli)
  COCLI_TARGET        Override Rust target triple
  GITHUB_TOKEN        Optional token for GitHub API / asset downloads

Binaries install to $COCLI_PREFIX/bin. The data directory is not modified.

Next action after install: run `cocli`, then open http://127.0.0.1:8090
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi
if [[ $# -gt 0 ]]; then
  echo "install.sh: unexpected argument: $*" >&2
  usage >&2
  exit 2
fi

die() {
  echo "install.sh: $*" >&2
  exit 1
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

file_sha256() {
  local path=$1
  local hash
  if command -v sha256sum >/dev/null 2>&1; then
    hash=$(sha256sum "$path" | awk '{print $1}')
  elif command -v shasum >/dev/null 2>&1; then
    hash=$(shasum -a 256 "$path" | awk '{print $1}')
  elif command -v openssl >/dev/null 2>&1; then
    hash=$(openssl dgst -sha256 "$path" | awk '{print $NF}')
  else
    die "need sha256sum, shasum, or openssl"
  fi
  printf '%s' "$hash" | tr '[:upper:]' '[:lower:]'
}

# Look up a basename in GNU SHA256SUMS (hash, two spaces or " *", filename).
sums_hash_for() {
  local sums=$1
  local name=$2
  awk -v f="$name" '
    NF >= 2 {
      hash = $1
      file = $2
      sub(/^\*/, "", file)
      if (file == f || file == "./" f) {
        print hash
        exit
      }
    }
  ' "$sums" | tr '[:upper:]' '[:lower:]'
}

verify_named_files() {
  local sums=$1
  shift
  local name expected actual
  [[ -f "$sums" ]] || die "checksum file not found: $sums"
  for name in "$@"; do
    [[ -f "$name" ]] || die "missing artifact: $name"
    expected=$(sums_hash_for "$sums" "$(basename "$name")")
    [[ -n "$expected" ]] || die "SHA256SUMS has no entry for $(basename "$name")"
    actual=$(file_sha256 "$name")
    [[ "$actual" == "$expected" ]] || die "SHA-256 mismatch for $(basename "$name") (expected $expected, got $actual)"
  done
}

detect_target() {
  local sys mach
  sys=$(uname -s)
  mach=$(uname -m)
  case "$mach" in
    x86_64 | amd64) mach=x86_64 ;;
    arm64 | aarch64) mach=aarch64 ;;
    *) die "unsupported architecture: $mach" ;;
  esac
  case "$sys" in
    Darwin) echo "${mach}-apple-darwin" ;;
    Linux) echo "${mach}-unknown-linux-gnu" ;;
    MINGW* | MSYS* | CYGWIN*) echo "${mach}-pc-windows-msvc" ;;
    *) die "unsupported OS: $sys" ;;
  esac
}

normalize_version() {
  local raw=$1
  raw=${raw#v}
  [[ -n "$raw" ]] || die "empty COCLI_VERSION"
  printf '%s' "$raw"
}

curl_fetch() {
  local url=$1
  local out=$2
  local args=(-fsSL --retry 3 --retry-delay 1 -A "cocli-install")
  local token=${GITHUB_TOKEN:-${GH_TOKEN:-}}
  if [[ -n "$token" ]]; then
    args+=("-H" "Authorization: Bearer ${token}")
  fi
  curl "${args[@]}" -o "$out" "$url"
}

latest_tag() {
  local repo=$1
  local tmp tag
  need_cmd python3
  tmp=$(mktemp "${TMPDIR:-/tmp}/cocli-release.XXXXXX")
  if ! curl_fetch "https://api.github.com/repos/${repo}/releases/latest" "$tmp"; then
    rm -f "$tmp"
    return 1
  fi
  tag=$(python3 -c 'import json,sys
p=sys.argv[1]
try:
    data=json.load(open(p))
except Exception:
    sys.exit(1)
if not isinstance(data, dict) or data.get("message")=="Not Found" or "tag_name" not in data:
    sys.exit(1)
print(data["tag_name"])
' "$tmp") || {
    rm -f "$tmp"
    return 1
  }
  rm -f "$tmp"
  printf '%s' "$tag"
}

archive_name_for() {
  local version=$1
  local target=$2
  case "$target" in
    *-pc-windows-msvc) echo "cocli-${version}-${target}.zip" ;;
    *) echo "cocli-${version}-${target}.tar.gz" ;;
  esac
}

extract_archive() {
  local archive=$1
  local dest=$2
  case "$archive" in
    *.zip)
      need_cmd unzip
      unzip -q "$archive" -d "$dest"
      ;;
    *)
      tar -xzf "$archive" -C "$dest"
      ;;
  esac
}

find_binary() {
  local root=$1
  local name=$2
  local match path
  if [[ -f "$root/$name" ]]; then
    echo "$root/$name"
    return 0
  fi
  match=""
  while IFS= read -r path; do
    match=$path
    break
  done < <(find "$root" -type f -name "$name" ! -path '*/.*')
  [[ -n "$match" ]] || return 1
  echo "$match"
}

# Install src -> dest atomically. On failure, dest is left as it was.
# Previous dest is kept at dest.prev until the caller deletes it.
install_one() {
  local src=$1
  local dest=$2
  local dest_dir tmp
  dest_dir=$(dirname "$dest")
  mkdir -p "$dest_dir" || return 1
  tmp=$(mktemp "$dest_dir/.cocli-new.XXXXXX") || return 1
  # Explicit checks: this function is invoked from `if`, so set -e is off.
  if ! cp "$src" "$tmp"; then
    rm -f "$tmp"
    return 1
  fi
  if ! chmod 755 "$tmp"; then
    rm -f "$tmp"
    return 1
  fi
  if [[ -e "$dest" || -L "$dest" ]]; then
    if ! mv "$dest" "${dest}.prev"; then
      rm -f "$tmp"
      return 1
    fi
  fi
  if ! mv "$tmp" "$dest"; then
    rm -f "$tmp"
    if [[ -e "${dest}.prev" || -L "${dest}.prev" ]]; then
      mv "${dest}.prev" "$dest"
    fi
    return 1
  fi
}

# Roll back names successfully installed in this run. Restore dest.prev when
# present (upgrade); otherwise remove dest (first install had no previous).
rollback_binaries() {
  local bin_dir=$1
  shift
  local name dest
  for name in "$@"; do
    dest="$bin_dir/$name"
    if [[ -e "${dest}.prev" || -L "${dest}.prev" ]]; then
      rm -f "$dest"
      mv "${dest}.prev" "$dest"
    elif [[ -e "$dest" || -L "$dest" ]]; then
      rm -f "$dest"
    fi
  done
}

clear_prev() {
  local bin_dir=$1
  shift
  local name
  for name in "$@"; do
    rm -f "$bin_dir/${name}.prev"
  done
}

print_next() {
  local cocli_path=$1
  local bridge_path=$2
  local cocli_ver bridge_ver
  cocli_ver=$("$cocli_path" --version)
  bridge_ver=$("$bridge_path" --version)
  echo
  echo "Installed:"
  echo "  $cocli_ver -> $cocli_path"
  echo "  $bridge_ver -> $bridge_path"
  echo
  echo "Next: run \`cocli\`, then open ${UI_URL}"
}

maybe_path_note() {
  local bin_dir=$1
  case ":$PATH:" in
    *":${bin_dir}:"*) ;;
    *)
      echo
      echo "Note: ${bin_dir} is not on PATH. Add it before running \`cocli\`."
      ;;
  esac
}

WORK=$(mktemp -d "${TMPDIR:-/tmp}/cocli-install.XXXXXX")
cleanup() {
  rm -rf "$WORK"
}
trap cleanup EXIT

TARGET=${COCLI_TARGET:-$(detect_target)}
PREFIX=${COCLI_PREFIX:-"${HOME:?HOME is required}/.local"}
REPO=${COCLI_REPO:-$REPO_DEFAULT}
BIN_DIR="${PREFIX}/bin"
BINARIES=(cocli cocli-bridge)

case "$TARGET" in
  *-pc-windows-msvc)
    BINARIES=(cocli.exe cocli-bridge.exe)
    ;;
esac

mkdir -p "$WORK/stage"

if [[ -n "${COCLI_ARTIFACT_DIR:-}" ]]; then
  ARTIFACT_DIR=$COCLI_ARTIFACT_DIR
  [[ -d "$ARTIFACT_DIR" ]] || die "COCLI_ARTIFACT_DIR is not a directory: $ARTIFACT_DIR"
  ARTIFACT_DIR=$(cd "$ARTIFACT_DIR" && pwd -P)
  [[ -f "$ARTIFACT_DIR/SHA256SUMS" ]] || die "COCLI_ARTIFACT_DIR has no SHA256SUMS. Generate it with: scripts/write-sha256sums.sh \"$ARTIFACT_DIR\" ${BINARIES[*]}"

  for name in "${BINARIES[@]}"; do
    [[ -f "$ARTIFACT_DIR/$name" ]] || die "COCLI_ARTIFACT_DIR missing $name"
    cp "$ARTIFACT_DIR/$name" "$WORK/stage/$name"
  done
  cp "$ARTIFACT_DIR/SHA256SUMS" "$WORK/SHA256SUMS"
  (
    cd "$WORK/stage"
    verify_named_files "$WORK/SHA256SUMS" "${BINARIES[@]}"
  )
else
  need_cmd curl
  VERSION_RAW=${COCLI_VERSION:-}
  if [[ -z "$VERSION_RAW" ]]; then
    VERSION_RAW=$(latest_tag "$REPO") || die "no GitHub release found for ${REPO}. Build locally and set COCLI_ARTIFACT_DIR (see scripts/write-sha256sums.sh)."
  fi
  VERSION=$(normalize_version "$VERSION_RAW")
  TAG="v${VERSION}"
  ARCHIVE=$(archive_name_for "$VERSION" "$TARGET")
  BASE="https://github.com/${REPO}/releases/download/${TAG}"

  curl_fetch "${BASE}/SHA256SUMS" "$WORK/SHA256SUMS" || die "failed to download SHA256SUMS from ${BASE}/SHA256SUMS"
  curl_fetch "${BASE}/${ARCHIVE}" "$WORK/${ARCHIVE}" || die "failed to download ${ARCHIVE} from ${BASE}/${ARCHIVE}"
  (
    cd "$WORK"
    verify_named_files "$WORK/SHA256SUMS" "$ARCHIVE"
  )
  mkdir -p "$WORK/extract"
  extract_archive "$WORK/${ARCHIVE}" "$WORK/extract"
  for name in "${BINARIES[@]}"; do
    found=$(find_binary "$WORK/extract" "$name") || die "archive ${ARCHIVE} does not contain $name"
    cp "$found" "$WORK/stage/$name"
  done
  # If the checksum file also lists the inner binaries, verify them too.
  missing_inner=0
  for name in "${BINARIES[@]}"; do
    if [[ -z "$(sums_hash_for "$WORK/SHA256SUMS" "$name")" ]]; then
      missing_inner=1
      break
    fi
  done
  if [[ "$missing_inner" -eq 0 ]]; then
    (
      cd "$WORK/stage"
      verify_named_files "$WORK/SHA256SUMS" "${BINARIES[@]}"
    )
  fi
fi

# Re-verify staged copies immediately before replace.
(
  cd "$WORK/stage"
  # When SHA256SUMS only has the archive, hash the staged files against themselves
  # by requiring the copies still match the verified sources via a local sums file.
  if [[ -n "$(sums_hash_for "$WORK/SHA256SUMS" "${BINARIES[0]}")" ]]; then
    verify_named_files "$WORK/SHA256SUMS" "${BINARIES[@]}"
  else
    : # archive-level checksum already verified the payload
  fi
)

mkdir -p "$BIN_DIR"
[[ -w "$BIN_DIR" ]] || die "install prefix is not writable (no admin by default): $BIN_DIR"

installed=()
for name in "${BINARIES[@]}"; do
  if ! install_one "$WORK/stage/$name" "$BIN_DIR/$name"; then
    if [[ ${#installed[@]} -gt 0 ]]; then
      rollback_binaries "$BIN_DIR" "${installed[@]}"
    fi
    die "failed to install $name; destination restored to previous state"
  fi
  installed+=("$name")
done
clear_prev "$BIN_DIR" "${BINARIES[@]}"

print_next "$BIN_DIR/${BINARIES[0]}" "$BIN_DIR/${BINARIES[1]}"
maybe_path_note "$BIN_DIR"

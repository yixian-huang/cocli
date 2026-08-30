#!/usr/bin/env bash
# Write GNU-style SHA256SUMS for local installer artifacts.
#
# Usage:
#   scripts/write-sha256sums.sh <dir> [file...]
#
# With no file list, hashes cocli / cocli-bridge (and Windows .exe names)
# when those files exist in <dir>. Overwrites <dir>/SHA256SUMS.
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: write-sha256sums.sh <dir> [file...]

Write <dir>/SHA256SUMS in GNU `sha256sum` format (hash, two spaces, filename).
Used by scripts/install.sh via COCLI_ARTIFACT_DIR until GitHub Releases exist.
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if [[ $# -lt 1 ]]; then
  usage >&2
  exit 2
fi

dir=$1
shift

if [[ ! -d "$dir" ]]; then
  echo "write-sha256sums: not a directory: $dir" >&2
  exit 1
fi

dir=$(cd "$dir" && pwd -P)

files=("$@")
if [[ ${#files[@]} -eq 0 ]]; then
  for candidate in cocli cocli-bridge cocli.exe cocli-bridge.exe; do
    if [[ -f "$dir/$candidate" ]]; then
      files+=("$candidate")
    fi
  done
fi

if [[ ${#files[@]} -eq 0 ]]; then
  echo "write-sha256sums: no files to hash in $dir" >&2
  exit 1
fi

for file in "${files[@]}"; do
  if [[ "$file" == */* || "$file" == SHA256SUMS ]]; then
    echo "write-sha256sums: hash basename artifacts only: $file" >&2
    exit 1
  fi
  if [[ ! -f "$dir/$file" ]]; then
    echo "write-sha256sums: missing $dir/$file" >&2
    exit 1
  fi
done

hash_file() {
  local path=$1
  local hash
  if command -v sha256sum >/dev/null 2>&1; then
    hash=$(sha256sum "$path" | awk '{print $1}')
  elif command -v shasum >/dev/null 2>&1; then
    hash=$(shasum -a 256 "$path" | awk '{print $1}')
  elif command -v openssl >/dev/null 2>&1; then
    hash=$(openssl dgst -sha256 "$path" | awk '{print $NF}')
  else
    echo "write-sha256sums: need sha256sum, shasum, or openssl" >&2
    return 1
  fi
  printf '%s' "$hash" | tr '[:upper:]' '[:lower:]'
}

tmp=$(mktemp "${TMPDIR:-/tmp}/cocli-sha256sums.XXXXXX")
cleanup() {
  rm -f "$tmp"
}
trap cleanup EXIT

: >"$tmp"
for file in "${files[@]}"; do
  hash=$(hash_file "$dir/$file")
  hash=$(printf '%s' "$hash" | tr '[:upper:]' '[:lower:]')
  printf '%s  %s\n' "$hash" "$file" >>"$tmp"
done

mv "$tmp" "$dir/SHA256SUMS"
trap - EXIT
echo "wrote $dir/SHA256SUMS"
cat "$dir/SHA256SUMS"

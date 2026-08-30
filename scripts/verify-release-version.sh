#!/usr/bin/env bash
# Verify release tag / workflow_dispatch against the workspace version.
#
# Tag push v0.0.*: tag (without v) must match [workspace.package] version.
# workflow_dispatch: builds artifacts from Cargo.toml; does not create or move
# a tag (draft GitHub Release is tag-push only).
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: verify-release-version.sh

Reads GitHub Actions env (GITHUB_EVENT_NAME, GITHUB_REF_TYPE, GITHUB_REF_NAME).
Requires Cargo.lock and [workspace.package] version in Cargo.toml.

Writes version=, tag=, and publish_release= (true only on a matching v0.0.*
tag push) to GITHUB_OUTPUT when that file is set.
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi
if [[ $# -gt 0 ]]; then
  echo "verify-release-version: unexpected argument: $*" >&2
  usage >&2
  exit 2
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$ROOT"

die() {
  echo "verify-release-version: $*" >&2
  exit 1
}

[[ -f Cargo.lock ]] || die "Cargo.lock is missing"
[[ -f Cargo.toml ]] || die "Cargo.toml is missing"

python_bin=""
if command -v python3 >/dev/null 2>&1; then
  python_bin=python3
elif command -v python >/dev/null 2>&1; then
  python_bin=python
else
  die "python3 or python is required"
fi

# python -c (not a heredoc inside $()) so macOS /bin/bash 3.2 can parse this.
version=$("$python_bin" -c 'import re, pathlib, sys
text = pathlib.Path("Cargo.toml").read_text()
match = re.search(r"(?ms)^\[workspace\.package\].*?^version\s*=\s*\"([^\"]+)\"", text)
if not match:
    sys.exit("workspace.package version not found in Cargo.toml")
print(match.group(1))
')
[[ -n "$version" ]] || die "empty workspace version"

event=${GITHUB_EVENT_NAME:-}
ref_type=${GITHUB_REF_TYPE:-}
ref_name=${GITHUB_REF_NAME:-}
tag=""
publish_release=false

case "$event" in
  push)
    [[ "$ref_type" == "tag" ]] || die "push event is not a tag (ref_type=${ref_type})"
    case "$ref_name" in
      v0.0.*) ;;
      *) die "tag ${ref_name} is not v0.0.*" ;;
    esac
    tag=$ref_name
    tag_version=${ref_name#v}
    if [[ "$tag_version" != "$version" ]]; then
      die "tag ${ref_name} does not match workspace version ${version}"
    fi
    echo "tag ${tag} matches workspace version ${version}"
    publish_release=true
    ;;
  workflow_dispatch)
    echo "workflow_dispatch: building artifacts only; will not create or move tag v${version}"
    tag="v${version}"
    publish_release=false
    ;;
  "")
    echo "no GitHub event: using workspace version ${version} (local check)"
    tag="v${version}"
    publish_release=false
    ;;
  *)
    die "unsupported event ${event} (expected tag push v0.0.* or workflow_dispatch)"
    ;;
esac

echo "version=${version}"
echo "tag=${tag}"
echo "publish_release=${publish_release}"

if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  {
    echo "version=${version}"
    echo "tag=${tag}"
    echo "publish_release=${publish_release}"
  } >>"$GITHUB_OUTPUT"
fi

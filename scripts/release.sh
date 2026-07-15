#!/usr/bin/env bash
# Cut a dyndo release: bump all workspace crate versions in lockstep, commit,
# tag, and push. Pushing the tag triggers .github/workflows/release.yml, which
# verifies, builds, and publishes the GitHub Release.
#
# Usage: scripts/release.sh
set -euo pipefail

# Repo root (this script lives in scripts/).
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# dyndo-cli is the source-of-truth manifest for the current version.
CLI_MANIFEST="$ROOT/crates/dyndo-cli/Cargo.toml"
# All manifests bumped in lockstep.
MANIFESTS=(
  "$ROOT/crates/dyndo-core/Cargo.toml"
  "$ROOT/crates/dyndo-cli/Cargo.toml"
  "$ROOT/crates/dyndo-server/Cargo.toml"
)

# Print the [package] version from a Cargo.toml (the first `version = "..."`).
read_current_version() {
  sed -n 's/^version = "\(.*\)"/\1/p' "$1" | head -n1
}

# Return 0 iff $1 is a bare X.Y.Z semver (numeric, no pre-release/build).
is_semver() {
  [[ "$1" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]
}

# Return 0 iff version $1 is strictly greater than version $2 (numeric compare).
version_gt() {
  local a1 a2 a3 b1 b2 b3
  IFS=. read -r a1 a2 a3 <<<"$1"
  IFS=. read -r b1 b2 b3 <<<"$2"
  if ((a1 != b1)); then ((a1 > b1)); return; fi
  if ((a2 != b2)); then ((a2 > b2)); return; fi
  ((a3 > b3))
}

# Rewrite only the [package] version line of a single manifest. Portable (no
# `sed -i`, which differs between GNU and BSD/macOS): the `1,/^version = /`
# range stops at the first version line, so dependency `version = "..."` entries
# further down are untouched.
bump_manifest() {
  local manifest="$1" version="$2" tmp
  tmp="$(mktemp)"
  sed "1,/^version = /s/^version = \".*\"/version = \"$version\"/" "$manifest" >"$tmp"
  mv "$tmp" "$manifest"
}

# Rewrite all workspace manifests to $1.
bump_manifests() {
  local m
  for m in "${MANIFESTS[@]}"; do
    bump_manifest "$m" "$1"
  done
}

main() {
  # Repo preconditions (fail fast, before prompting).
  [[ "$(git -C "$ROOT" rev-parse --abbrev-ref HEAD)" == "main" ]] \
    || { echo "error: not on main branch" >&2; exit 1; }
  [[ -z "$(git -C "$ROOT" status --porcelain)" ]] \
    || { echo "error: working tree is dirty; commit or stash first" >&2; exit 1; }

  local current
  current="$(read_current_version "$CLI_MANIFEST")"
  is_semver "$current" \
    || { echo "error: current version '$current' is not X.Y.Z" >&2; exit 1; }

  printf 'Current version: %s, next version: ' "$current"
  local next
  read -r next

  is_semver "$next" \
    || { echo "error: '$next' is not a valid X.Y.Z version" >&2; exit 1; }
  version_gt "$next" "$current" \
    || { echo "error: $next is not greater than current $current" >&2; exit 1; }

  local tag="v$next"
  git -C "$ROOT" rev-parse -q --verify "refs/tags/$tag" >/dev/null 2>&1 \
    && { echo "error: tag $tag already exists" >&2; exit 1; }

  # Ensure local main matches origin so the release is cut from pushed history.
  git -C "$ROOT" fetch --quiet origin main
  [[ "$(git -C "$ROOT" rev-parse HEAD)" == "$(git -C "$ROOT" rev-parse origin/main)" ]] \
    || { echo "error: local main differs from origin/main; pull/push first" >&2; exit 1; }

  bump_manifests "$next"
  ( cd "$ROOT" && cargo update --workspace --quiet )

  git -C "$ROOT" add "${MANIFESTS[@]}" "$ROOT/Cargo.lock"
  git -C "$ROOT" commit -m "release: $next"
  git -C "$ROOT" tag "$tag"
  git -C "$ROOT" push --follow-tags origin main

  echo "Pushed $tag. Track the release workflow under the repo's Actions tab."
}

main

#!/usr/bin/env bash
# Cut a release: bump version, build package, tag, push, and open a GitHub
# release draft. Run from the project root as: ./scripts/release.sh <major|minor|patch>
# This is invoked by the `release` just recipe, keeping the multi-line shell
# logic out of the justfile (just executes each recipe line in its own shell).

set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <major|minor|patch>" >&2
  exit 2
fi
target=$1

cd "$(dirname "$0")/.."   # project root (repo dir, where justfile lives)

# --- preflight: must be on main, clean, and in sync with upstream ---
branch=$(git rev-parse --abbrev-ref HEAD)
if [[ "$branch" != "main" ]]; then
  echo "error: releases must be cut from main (currently on '$branch')" >&2
  exit 2
fi
if ! git diff --quiet || ! git diff --cached --quiet; then
  echo "error: working tree has uncommitted changes; commit or stash first" >&2
  exit 2
fi
ahead=$(git rev-list --count @{u}..HEAD 2>/dev/null || echo err)
behind=$(git rev-list --count HEAD..@{u} 2>/dev/null || echo err)
if [[ "$ahead" != "0" || "$behind" != "0" ]]; then
  echo "error: not in sync with upstream (ahead: $ahead, behind: $behind); push/pull first" >&2
  exit 2
fi

# --- bump version (validates target), then build the package ---
just incver "$target"
just package

# --- commit, tag, push, and create the release draft ---
git add .
TAG="$(tomlq -r '.package.version' Cargo.toml)"
TAGV="v$TAG"
git commit -m "release $TAGV"
git push
git tag -a "$TAGV"
git push --tags
gh release create "$TAGV" \
  --title "unstable $TAGV" \
  --draft \
  --notes "feature list" \
  ./target/release/dfm \
  ./target/cargo-aur/dfm-bin-$TAG-1-x86_64.pkg.tar.zst \
  ./target/cargo-aur/PKGBUILD

#!/usr/bin/env bash
# Cut a release. With a version part (<major|minor|patch>) it bumps the version,
# rebuilds the package and creates a new release commit; without an argument it
# does NOT bump or commit — it rebuilds the package and (re)creates the release
# for the current Cargo.toml version. The no-argument form is the recovery path
# after a failed tag push, so the same version can be released without skipping
# it. Run from the project root as: ./scripts/release.sh [ <major|minor|patch> ]
# This is invoked by the `release` just recipe, keeping the multi-line shell
# logic out of the justfile (just executes each recipe line in its own shell).

set -euo pipefail

if [[ $# -gt 1 ]]; then
  echo "usage: $0 [ <major|minor|patch> ]" >&2
  exit 2
fi
target=${1:-}

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

# Remote is derived from the upstream (e.g. "origin") and used for the
# tag-existence check and the tag push below.
remote=$(git rev-parse --abbrev-ref --symbolic-full-name @{u} 2>/dev/null)
remote=${remote%%/*}

# --- bump version (validates target) then build the package ---
if [[ -n "$target" ]]; then
  just incver "$target"
fi
just package

TAG="$(tomlq -r '.package.version' Cargo.toml)"
TAGV="v$TAG"

# --- create the release commit and push it (only when the version was bumped) ---
if [[ -n "$target" ]]; then
  git add .
  git commit -m "release $TAGV"
  git push
fi

# --- tag conditionally, then push the tag only when it is missing on the remote ---
if git ls-remote --tags --exit-code "$remote" "$TAGV" > /dev/null 2>&1; then
  echo "tag $TAGV already exists on $remote; not re-creating or re-pushing"
elif git rev-parse -q --verify "refs/tags/$TAGV" > /dev/null 2>&1; then
  echo "tag $TAGV exists locally but not on $remote; pushing it"
  git push "$remote" "$TAGV"
else
  git tag -a "$TAGV"
  git push "$remote" "$TAGV"
fi

# --- create the GitHub release draft ---
gh release create "$TAGV" \
  --title "unstable $TAGV" \
  --draft \
  --notes "feature list" \
  ./target/release/dfm \
  ./target/cargo-aur/dfm-bin-$TAG-1-x86_64.pkg.tar.zst \
  ./target/cargo-aur/PKGBUILD

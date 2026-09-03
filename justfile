# Tasks for dfm development.

# Run integraion tests.
test:
  ./tests/launcher.sh

# Build only, no package.
build:
    cargo build

# Build package for Arch Linux pacman
package:
    CARGO_TARGET_DIR=/tmp/dfm-target cargo aur
    (cd ./target/cargo-aur && if [ ! find . -type f -name '*.pkg.tar.zst' 2>&1>/dev/null ]; then makepkg; fi)
    find -L . -type f -name '*.pkg.tar.zst'

# Build package and install via pacman
install:
    just package
    (cd ./target/cargo-aur && makepkg -si)
    echo "installed to $(which dfm)"

# Increment version. target is one of major/minor/patch.
incver target:
    # normalize target -> incsemver token and validate
    case "$target" in
    major|minor|patch) ;;
    *) echo "error: target must be major, minor or patch, got '$target'" >&2; exit 2 ;;
    esac
    CURRENT_VERSION=$(tomlq 'package.version' Cargo.toml)
    NEW_VERSION=$(./scripts/incsemver "$target" "$CURRENT_VERSION")
    tomlq "package.version = $NEW_VERSION" Cargo.toml

# Set tag, push it to git, build applicatoin and create a release draft at github.com
# Usage: just release major
release target:
    # ensure we are on main with nothing uncommitted and in sync with upstream
    branch=$(git rev-parse --abbrev-ref HEAD)
    if [ "$branch" != "main" ]; then
    echo "error: releases must be cut from main (currently on '$branch')" >&2
    exit 2
    fi
    if ! git diff --quiet || ! git diff --cached --quiet; then
    echo "error: working tree has uncommitted changes; commit or stash first" >&2
    exit 2
    fi
    ahead=$(git rev-list --count @{u}..HEAD 2>/dev/null || echo err)
    behind=$(git rev-list --count HEAD..@{u} 2>/dev/null || echo err)
    if [ "$ahead" != "0" ] || [ "$behind" != "0" ]; then
    echo "error: not in sync with upstream (ahead: $ahead, behind: $behind); push/pull first" >&2
    exit 2
    fi
    just incver "$target"
    just package
    git add .
    TAG="$(tomlq 'package.version' Cargo.toml)"
    TAGV="v$TAG"
    git commit -m "release $TAGV"
    git push
    git tag -a $TAGV
    git push --tags
    gh release create $TAGV --title "unstable $TAGV" --draft --notes "feature list" ./target/release/dfm ./target/cargo-aur/dfm-bin-$TAG-1-x86_64.pkg.tar.zst ./target/cargo-aur/PKGBUILD


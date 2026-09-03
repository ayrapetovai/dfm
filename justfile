# Tasks for dfm development.

# Run integraion tests.
test:
    ./tests/launcher.sh

# Build only, no package.
build:
    cargo build

# Build package for Arch Linux pacman.
package:
    CARGO_TARGET_DIR=/tmp/dfm-target cargo aur
    (cd ./target/cargo-aur && if [ ! find . -type f -name '*.pkg.tar.zst' 2>&1>/dev/null ]; then makepkg; fi)
    find -L . -type f -name '*.pkg.tar.zst'

# Build package and install via pacman.
install:
    just package
    (cd ./target/cargo-aur && makepkg -si)
    echo "installed to $(which dfm)"

# Increment version. Usage: just incver <major|minor|patch>
incver target:
    NV=$(./scripts/incsemver {{target}} "$(tomlq -r '.package.version' Cargo.toml)") && tomlq -i -t ".package.version = \"$NV\"" Cargo.toml

# Set tag, push it to git, build applicatoin and create a release draft at github.com. Usage: just release <major|minor|patch>
release target:
    ./scripts/release.sh {{target}}


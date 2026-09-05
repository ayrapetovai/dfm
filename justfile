# Tasks for dfm development.

# Build only, no package creating.
build:
    cargo build

# Delete build and package.
clean:
    cargo clean

# Run integraion tests.
test:
    ./tests/launcher.sh

# Build package for pacman.
package:
    CARGO_TARGET_DIR=/tmp/dfm-target cargo aur
    (cd ./target/cargo-aur && if find . -type f -name '*.pkg.tar.zst' | grep -q .; then :; else makepkg; fi)
    find -L . -type f -name '*.pkg.tar.zst'

# Build package and install via pacman.
install:
    just package
    (cd ./target/cargo-aur && makepkg -si)
    echo "installed to $(which dfm)"

# Increment version. Usage: just incver <major|minor|patch>
incver target:
    NV=$(./scripts/incsemver {{target}} "$(tomlq -r '.package.version' Cargo.toml)") && tomlq -i -t ".package.version = \"$NV\"" Cargo.toml

# Set tag, push it to git, build applicatoin and create a release draft at github.com.
# With a version part it bumps first; without one it re-releases the current
# version (recovery after a failed tag push). Usage: just release [ <major|minor|patch> ]
release target="":
    ./scripts/release.sh {{target}}


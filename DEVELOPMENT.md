# Development

## Task runner: `justfile`

`dfm` uses [`just`](https://github.com/casey/just) as its task runner, with all
recipes defined in `./justfile`. Running a recipe is `just <name>`; run `just
--list` (or just `just -l`) to see every recipe and its comment.

The recipes cover the day-to-day workflow. Most are thin wrappers that delegate
to a script under `./scripts/` instead of inlining multi-line shell, so the
justfile stays a readable index and the logic lives in a testable script:

- `just build` — `cargo build`, compile only (no package).
- `just clean` — `cargo clean`.
- `just test` — the `./tests/launcher.sh` integration suite.
- `just package` — build the pacman package via `cargo aur` + `makepkg`.
- `just install` — build the package and install it with `pacman -si`.
- `just install-bin` — release build + `install -D` to `/usr/bin/dfm` (root).
- `just incver <major|minor|patch>` — bump the version in `Cargo.toml`.
- `just release [ <major|minor|patch> ]` — bump (optional), tag, and open a
  draft GitHub release via `./scripts/release.sh`.

**Rule: any repeatable multi-step task must be scripted and added to
`./justfile` as a recipe.** If a task needs to be done more than once, it is a
candidate for a recipe; wrap its steps in a script under `./scripts/` (add the
shebang, make it executable, keep `set -euo pipefail`) and have the recipe call
it. This keeps commands reproducible, documented in one place, and out of
people's muscle memory.

## Build tools

The project uses a few external tools. They are called either directly by the
recipes above, by the scripts they invoke, or by `build.rs`:

- **`gh`** (GitHub CLI, https://cli.github.com/) — used by `./scripts/release.sh`
  (`gh release create`) to create and push the draft release with its assets.
  It must be installed and authenticated (`gh auth login`) for `just release`.
- **`yq`/`tomlq`** — used by the `incver` recipe and `./scripts/incsemver` to
  read and rewrite `Cargo.toml`'s `.package.version`. The recipes invoke the
  TOML-aware `tomlq` command, so it is surfaced here as **`tomlq`**
  (`tomlq -r '.package.version' Cargo.toml` for reading,
  `tomlq -i -t` for in-place writes). On the reference system `tomlq -V`
  reports `jq-1.8.2`, i.e. the Real Tomlq jq wrapper; ensure whichever `tomlq`
  is installed is on `PATH`.
- **`just`** — the task runner itself (see the section above).
- **`cargo aur`** (`cargo install cargo-aur`) — builds the pacman source
  package (`./target/cargo-aur`). A toolbox dependency, not from the
  GitHub CLI set.

## Building

### Install tools

Install https://rust-lang.org/tools/install/

```shell
cargo install cargo-aur
```

### Generate the man page

`build.rs` renders `dfm.1` on every build — no separate step needed. It is a
hybrid page: `clap_mangen` generates the CLI reference (NAME, SYNOPSIS,
OPTIONS, VERSION) from `src/cli.rs`, while `build/man.rs` (a dependency-free
`README.md` → roff converter, `#[path]`-included by the build script) supplies
the prose sections (DESCRIPTION plus every `##` section below it). The output
lands in `target/<profile>/dfm.1` under the effective `target-dir` (the repo
redirects it to `/tmp/dfm-target`). Since `cargo aur` first runs
`cargo build --release`, the man page lands in `target/release/dfm.1`, which
`[package.metadata.aur] files` packs into the release tarball and the PKGBUILD
installs to `/usr/share/man/man1/dfm.1`.

`build.rs` re-runs when any of `build.rs`, `build/man.rs`, `src/cli.rs`, or
`README.md` changes. To inspect the rendered page run
`groff -man -t -Tutf8 target/<profile>/dfm.1` (or `man -l`).

### Create a package from sources

```shell
# from project root
cargo aur
cd ./target/cargo-aur
makepkg
```

The package will appear in ./target/cargo-aur

### Install or remove the package

```shell
# install
sudo pacman -U dfm-bin-0.0.0-1-x86_64.pkg.tar.zst

# remove
sudo pacman -R dfm-bin
```

### Install the binary to /usr/bin/dfm with `install`

```shell
just install-bin    # needs root: sudo just install-bin
```

Builds a release binary and installs it to `/usr/bin/dfm` with the `install`
utility (mode 0755), without building a package:

```shell
cargo build --release
install -D -m 0755 target/release/dfm /usr/bin/dfm
```

The man page is not installed by this task — use the pacman package
(`just install`) if `/usr/share/man/man1/dfm.1` is needed.

## Create a release

Rise version in Cargo.toml, than create tag and draft release.

```shell
export NEW_TAG=0.0.0
export NEW_TAGV=v"$NEW_TAG"
cargo aur                          # builds release + target/release/dfm.1, then tarball
makepkg -C ../target/cargo-aur     # build the .pkg.tar.zst
git push
git tag -a $NEW_TAGV
git push --tags
gh release create $NEW_TAGV --title "unstable $NEW_TAGV" --draft --notes "feature list" ./target/release/dfm ./target/cargo-aur/dfm-bin-$NEW_TAG-1-x86_64.pkg.tar.zst ./target/cargo-aur/PKGBUILD
```

The GitHub `source=` in the PKGBUILD points at the
`dfm-<version>-x86_64.tar.gz` **release asset**, so it must be uploaded with the
release for `makepkg` to download it.

At the release page edit the release notes: add feature/fix list.
Than publish the release.


# Development

## Building

### Install tools

Install https://rust-lang.org/tools/install/

```shell
cargo install cargo-aur
```

### Generate the man page

`build.rs` renders `dfm.1` from the CLI definition (via `clap_mangen`) into
`target/<profile>/dfm.1` on every build — no separate step needed. Since
`cargo aur` first runs `cargo build --release`, the man page lands in
`target/release/dfm.1`, which `[package.metadata.aur] files` packs into the
release tarball and the PKGBUILD installs to `/usr/share/man/man1/dfm.1`.

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


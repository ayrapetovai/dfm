# Development

## Building

### Install tools

Install https://rust-lang.org/tools/install/

```shell
cargo install cargo-aur
```

### Generate the man page

The man page is generated from the CLI definition with `clap_mangen`:

```shell
# from project root
cargo run --example gen-manpage
```

This writes `dfm.1` to the project root. It must exist **before** running
`cargo aur` below: `[package.metadata.aur] files` packs it into the release
tarball and the PKGBUILD installs it to `/usr/share/man/man1/dfm.1`.

### Create a package from sources

```shell
# from project root
cargo run --example gen-manpage   # regenerate dfm.1 (required, see above)
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

## Create a release

Rise version in Cargo.toml, than create tag and draft release.

```shell
cargo run --example gen-manpage   # regenerate dfm.1 to match the new version
cargo aur                          # rebuild ./target/cargo-aur tarball + PKGBUILD
makepkg -C ../target/cargo-aur     # build the .pkg.tar.zst
git push
git tag -a v0.0.0
git push --tags
gh release create v0.0.0 --title "release v0.0.0" --draft --notes "unstable" ./target/release/dfm ./target/cargo-aur/dfm-0.0.0-x86_64.tar.gz ./target/cargo-aur/PKGBUILD
```

The GitHub `source=` in the PKGBUILD points at the
`dfm-<version>-x86_64.tar.gz` **release asset**, so it must be uploaded with the
release for `makepkg` to download it.

At the release page edit the release notes: add feature/fix list.
Than publish the release.


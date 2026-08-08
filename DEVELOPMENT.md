# Development

## Building

### Install tools

Install https://rust-lang.org/tools/install/

```shell
cargo install cargo-aur
```

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

## Create a release

Rise version in Cargo.toml, than create tag and draft release.

```shell
git push
git tag -a v0.0.0
git push --tags
gh release create v0.0.0 --title "release v0.0.0" --draft --notes "unstable" ./target/release/dfm ./target/cargo-aur/dfm-bin-0.0.0-1-x86_64.pkg.tar.zst
```

At the release page edit the release notes: add feature/fix list.
Than publish the release.


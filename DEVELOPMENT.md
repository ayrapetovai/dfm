# Development

## Create a release

Create tag and draft release.

```shell
git tag -a v0.0.0
git push --tags
gh release create v0.0.0 --title "release v0.0.0" --draft --notes "unstable" ./target/release/dfm ./target/cargo-aur/dfm-bin-0.0.0-1-x86_64.pkg.tar.zst
```

The publish the release from the release web page.

## Building

### Install tools
install https://rust-lang.org/tools/install/

```shell
cargo install cargo-aur
```

### Create a package from sources

```shell
cargo aur
cd target/cargo-aur
makepkg
```

The package will appear in ./target/cargo-aur

### Install or remove the package

```shell
# install
sudo pacman -U dfm-version-arch.zst

# remove
sudo pacman -R dfm-bin


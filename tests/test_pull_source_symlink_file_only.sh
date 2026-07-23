dfm init dotfiles

# create a source symlink file without a regular source file
mkdir -p "$PWD/dotfiles"
echo "/some/external/path" > "$PWD/dotfiles/link_target.symlink"

# remove any existing target (should not exist)
rm -f link_target

# pull all — the .symlink file should be picked up and a target symlink created
dfm pull

# postcondition: target symlink was created from the .symlink file
assert -L link_target
assert "/some/external/path" = "$(readlink link_target)"

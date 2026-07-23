dfm init dotfiles

# create a symlink
mkdir -p real_files
echo "content" > "real_files/target.txt"
ln -s "real_files/target.txt" "mylink"

# add the symlink first (so it exists in source)
dfm add mylink
assert -f "$PWD/dotfiles/mylink.symlink"

# dfm add -e mylink without --force → must fail (cannot encrypt a symlink)
assert_fail dfm add -e mylink

# dfm add -e mylink WITH --force → succeeds (logs error but forces through)
dfm add -f -e mylink

# postcondition: existing symlink file is unchanged
assert -f "$PWD/dotfiles/mylink.symlink"
assert "real_files/target.txt" = "$(cat "$PWD/dotfiles/mylink.symlink")"

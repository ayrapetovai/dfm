# add --force must override the ignore pattern for an ignored symlink:
# the symlink pointer file is created and the pattern is pruned, so a later
# plain `add` no longer skips it.

dfm init dotfiles

mkdir -p real_files
echo "content" >"real_files/target.txt"
ln -s "real_files/target.txt" "mylink"
dfm ignore mylink

# default add — must skip the ignored symlink silently
dfm add mylink 2>/dev/null
assert_no_source "mylink.symlink"

# add --force — must create the symlink pointer file
dfm add --force mylink 2>/dev/null
assert_source "mylink.symlink"

# the ignore pattern must be gone: a plain re-add must not skip again
rm "$PWD/dotfiles/mylink.symlink"
dfm add mylink 2>/dev/null
assert_source "mylink.symlink"


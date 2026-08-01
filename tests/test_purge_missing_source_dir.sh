CONTENT="$(uuid)"
# dfm purge must still succeed when the source directory is missing:
# config and state are removed, and the already-missing source is skipped.

dfm init dotfiles
write "$CONTENT" file.txt
dfm add file.txt

# remove the source directory entirely
rm -rf "$PWD/dotfiles"

dfm purge

# postconditions: config and state removed, source still absent
assert_fail test -f "$PWD/.config/dfm/config.toml"
assert_fail test -d "$PWD/.config/dfm"
assert_fail test -d "$PWD/.local/state/dfm"
assert_fail test -d "$PWD/dotfiles"

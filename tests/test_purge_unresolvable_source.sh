# purge must not fail when the source directory path cannot be resolved
# (state file unreadable -> source_dir resolves to empty). It must still
# delete config and state, and skip the unresolvable source directory.

CONTENT="$(uuid)"

dfm init dotfiles
write "$CONTENT" file.txt
dfm add file.txt

assert -f "$XDG_CONFIG_HOME/dfm/config.toml"
assert -d "$XDG_STATE_HOME/dfm"
assert -d "$PWD/dotfiles"
assert_source "file.txt"

# corrupt the state file so source_dir cannot be derived from it
echo "not [ valid toml" >"$XDG_STATE_HOME/dfm/state.toml"

dfm -v 2 purge 2>stderr.txt

# config and state were still deleted
assert_fail test -f "$XDG_CONFIG_HOME/dfm/config.toml"
assert_fail test -d "$XDG_STATE_HOME/dfm"

# source directory was skipped (path unresolvable), so it remains
assert -d "$PWD/dotfiles"
assert_source "file.txt"

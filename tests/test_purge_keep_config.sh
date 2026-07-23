CONTENT="$(uuid)"

dfm init dotfiles
write "$CONTENT" file.txt
dfm add file.txt

# preconditions: config, state, source dir all exist
assert -f "$PWD/.config/dfm/config.toml"
assert -d "$PWD/.local/state/dfm"
assert -d "$PWD/dotfiles"
assert_source "file.txt"

dfm purge --keep-config-file

# postconditions: config file still present
assert -f "$PWD/.config/dfm/config.toml"

# source directory and state are removed
assert_fail test -d "$PWD/dotfiles"
assert_fail test -d "$PWD/.local/state/dfm"

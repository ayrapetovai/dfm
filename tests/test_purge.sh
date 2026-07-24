CONTENT="$(uuid)"

dfm init dotfiles
write "$CONTENT" file.txt
dfm add file.txt

# preconditions: config, state, source dir all exist
assert -f "$PWD/.config/dfm/config.toml"
assert -d "$PWD/.local/state/dfm"
assert -d "$PWD/dotfiles"
assert_source "file.txt"

# purge without --force must fail when source has un-pulled changes
touch "$PWD/dotfiles/file.txt"
assert_fail dfm purge 2>/dev/null

# purge with --dry-run must not remove anything
dfm purge --dry-run
assert -f "$PWD/.config/dfm/config.toml"
assert -d "$PWD/.local/state/dfm"
assert -d "$PWD/dotfiles"
assert_source "file.txt"

# purge with --force succeeds despite changes
dfm purge --force

# postconditions: everything removed
assert_fail test -f "$PWD/.config/dfm/config.toml"
assert_fail test -d "$PWD/.local/state/dfm"
assert_fail test -d "$PWD/dotfiles"

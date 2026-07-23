CONTENT="$(uuid)"

dfm init dotfiles
write "$CONTENT" file.txt
dfm add file.txt

# preconditions: config, state, source dir all exist
assert -f "$PWD/.config/dfm/config.toml"
assert -d "$PWD/.local/state/dfm"
assert -d "$PWD/dotfiles"
assert_source "file.txt"

dfm purge --keep-source

# postconditions: source directory still present with the file
assert -d "$PWD/dotfiles"
assert_source "file.txt"
assert_content_eq "$PWD/dotfiles/file.txt" "$CONTENT"

# config and state are removed
assert_fail test -f "$PWD/.config/dfm/config.toml"
assert_fail test -d "$PWD/.local/state/dfm"

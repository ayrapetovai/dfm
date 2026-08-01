CONTENT="$(uuid)"

dfm init dotfiles
write "$CONTENT" file.txt
dfm add file.txt

# modify both the target and the source
write "modified" file.txt
touch "$PWD/dotfiles/file.txt"

# purge must fail because both sides have un-synced changes
assert_fail dfm purge

# nothing must be removed
assert -f "$PWD/.config/dfm/config.toml"
assert -d "$PWD/.config/dfm"
assert -d "$PWD/.local/state/dfm"
assert -d "$PWD/dotfiles"
assert_source "file.txt"

# purge --force must succeed despite the un-synced changes
dfm purge --force

# everything removed
assert_fail test -d "$PWD/dotfiles"
assert_fail test -d "$PWD/.config/dfm"
assert_fail test -d "$PWD/.local/state/dfm"

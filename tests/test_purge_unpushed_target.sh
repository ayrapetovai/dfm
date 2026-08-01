CONTENT="$(uuid)"

dfm init dotfiles
write "$CONTENT" file.txt
dfm add file.txt

# modify only the target file, source stays untouched
write "modified" file.txt

# purge must fail because the target has un-pushed changes
assert_fail dfm purge

# nothing must be removed
assert -f "$PWD/.config/dfm/config.toml"
assert -d "$PWD/.config/dfm"
assert -d "$PWD/.local/state/dfm"
assert -d "$PWD/dotfiles"
assert_source "file.txt"
assert_content_eq "file.txt" "modified"

# purge --force must succeed despite the un-pushed changes
dfm purge --force

# target file must remain with the modified content
assert_content_eq "file.txt" "modified"

# everything else removed
assert_fail test -d "$PWD/dotfiles"
assert_fail test -d "$PWD/.config/dfm"
assert_fail test -d "$PWD/.local/state/dfm"

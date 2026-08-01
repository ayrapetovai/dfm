CONTENT="$(uuid)"

dfm init dotfiles
write "$CONTENT" file.txt
dfm add file.txt
rm file.txt
dfm pull -s

# precondition: target file.txt is a managed symlink into the source dir
assert -L file.txt
assert "$PWD/dotfiles/file.txt" = "$(readlink -f file.txt)"

# purge must replace the symlink with a regular copy of its pointee
dfm purge

# postconditions: symlink replaced by a regular file with the same content
assert_fail test -L file.txt
assert_content_eq "file.txt" "$CONTENT"

# everything else removed
assert_fail test -d "$PWD/dotfiles"
assert_fail test -d "$PWD/.config/dfm"
assert_fail test -d "$PWD/.local/state/dfm"

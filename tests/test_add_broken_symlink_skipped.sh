# A broken symlink anywhere in the tree must not abort `dfm add .`; the
# broken link is skipped with a warning. Naming it explicitly still errors.

dfm init dotfiles

write "content" "file.txt"
ln -s "$PWD/missing-target" broken-link

# traversal-based add skips the broken symlink and still manages the good file
dfm add .
assert_source "file.txt"
assert_no_source "broken-link"

# explicitly naming the broken symlink is still an error
assert_fail dfm add broken-link

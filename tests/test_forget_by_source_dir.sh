# B3a2 — forget by providing a path inside the source directory (non-symlink file)
dfm init dotfiles

write "content" file.txt
dfm add file.txt

# forget by the source directory absolute path
dfm forget "$PWD/dotfiles/file.txt"

# source file must be removed
assert_fail test -f "$PWD/dotfiles/file.txt"

# target file must still exist
assert -f file.txt
assert "content" = "$(cat file.txt)"

# B4e — source file doesn't exist (manually removed from source dir)
dfm init dotfiles

CONTENT="$(uuid)"
write "$CONTENT" file.txt
dfm add file.txt

# manually remove the source file
rm "$PWD/dotfiles/file.txt"

# forget should succeed — source already gone
dfm forget file.txt

# target file must still exist
assert -f file.txt
assert "$CONTENT" = "$(cat file.txt)"

# pull must not recreate the source
dfm pull
assert_fail test -f "$PWD/dotfiles/file.txt"

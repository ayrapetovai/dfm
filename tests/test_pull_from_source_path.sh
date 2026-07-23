CONTENT="$(uuid)"

dfm init dotfiles
write "$CONTENT" file.txt
dfm add file.txt

# remove target file
rm file.txt
assert_fail test -f file.txt

# pull by specifying the source path
dfm pull "$PWD/dotfiles/file.txt"

# postcondition: target file was restored from source
assert -f file.txt
assert "$CONTENT" = "$(cat file.txt)"

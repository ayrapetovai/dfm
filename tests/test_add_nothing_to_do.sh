dfm init dotfiles

# run add when there are no files to manage → should succeed with "nothing to do"
dfm add

# now create and add a single file
CONTENT="$(uuid)"
write "$CONTENT" file.txt
dfm add file.txt

# add the same file again — already managed and unchanged → "nothing to do"
dfm add file.txt

# postcondition: file still exists with the same content
assert -f "$PWD/dotfiles/file.txt"
assert "$CONTENT" = "$(cat "$PWD/dotfiles/file.txt")"

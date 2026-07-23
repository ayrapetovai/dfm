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
assert_source "file.txt"
assert_content_eq "$PWD/dotfiles/file.txt" "$CONTENT"

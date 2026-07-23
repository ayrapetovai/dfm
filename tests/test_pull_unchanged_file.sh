CONTENT="$(uuid)"

dfm init dotfiles
write "$CONTENT" file.txt
dfm add file.txt

# pull the same file again — nothing has changed
dfm pull

# postcondition: target file still exists with the same content
assert -f file.txt
assert_content_eq "file.txt" "$CONTENT"

ORIGINAL="$(uuid)"
MODIFIED="$(uuid)"

dfm init dotfiles
write "$ORIGINAL" file.txt
dfm add file.txt
assert_content_eq "file.txt" "$ORIGINAL"

# modify the source file after add
write "$MODIFIED" "$PWD/dotfiles/file.txt"

# pull should detect that only the source was modified and apply the change
dfm pull

# postcondition: target file has the modified content from source
assert_content_eq "file.txt" "$MODIFIED"

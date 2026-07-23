ORIGINAL="$(uuid)"
MODIFIED="$(uuid)"

dfm init dotfiles
write "$ORIGINAL" file.txt
dfm add file.txt
assert "$ORIGINAL" = "$(cat file.txt)"

# modify the source file after add
write "$MODIFIED" "$PWD/dotfiles/file.txt"

# pull should detect that only the source was modified and apply the change
dfm pull

# postcondition: target file has the modified content from source
assert "$MODIFIED" = "$(cat file.txt)"

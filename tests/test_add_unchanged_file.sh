CONTENT="$(uuid)"

dfm init dotfiles
write "$CONTENT" file.txt
dfm add file.txt

# add the same file again — nothing has changed
dfm add file.txt

# postcondition: source file still exists with the same content
assert -f "$PWD/dotfiles/file.txt"
assert "$CONTENT" = "$(cat "$PWD/dotfiles/file.txt")"

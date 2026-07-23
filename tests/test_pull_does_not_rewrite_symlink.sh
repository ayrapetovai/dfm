OLD_CONTENT="$(uuid)"
NEW_CONTENT="$(uuid)"

dfm init dotfiles
write "$OLD_CONTENT" $PWD/dotfiles/file.txt
dfm pull -s
write "$NEW_CONTENT" $PWD/dotfiles/file.txt
assert_content_eq "file.txt" "$NEW_CONTENT"
dfm pull file.txt
assert -L file.txt
assert_content_eq "file.txt" "$NEW_CONTENT"

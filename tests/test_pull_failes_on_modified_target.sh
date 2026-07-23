OLD_CONTENT="$(uuid)"
NEW_CONTENT="$(uuid)"

dfm init dotfiles
write "$OLD_CONTENT" file.txt
dfm add file.txt
write "$NEW_CONTENT" file.txt
assert_fail dfm pull
assert_content_eq "file.txt" "$NEW_CONTENT"

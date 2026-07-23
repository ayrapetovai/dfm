OLD_CONTENT="$(uuid)"
NEW_CONTENT="$(uuid)"
ANOTHER_NEW_CONTENT="$(uuid)"

dfm init dotfiles
write "$OLD_CONTENT" file.txt
dfm add file.txt
write "$NEW_CONTENT" file.txt
# must fail becase target was modified
assert_fail dfm forget file.txt
rm file.txt

dfm pull
write "$ANOTHER_NEW_CONTENT" "$PWD/dotfiles/file.txt"
# must fail becase source is modified
assert_fail dfm forget file.txt
assert_source "file.txt"

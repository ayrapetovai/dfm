ORIGINAL_CONTENT="original"
NEW_CONTENT="new content"

dfm init dotfiles
write "$ORIGINAL_CONTENT" file.txt
dfm add file.txt

# modifed by some VCS
write "$NEW_CONTENT" ./dotfiles/file.txt
assert_fail dfm add file.txt

assert_content_eq "./dotfiles/file.txt" "$NEW_CONTENT"
assert_content_eq "file.txt" "$ORIGINAL_CONTENT"

dfm add -f file.txt
assert_content_eq "./dotfiles/file.txt" "$ORIGINAL_CONTENT"

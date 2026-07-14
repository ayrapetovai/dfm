ORIGINAL_CONTENT="original"
NEW_CONTENT="new content"

dfm init dotfiles
write "$ORIGINAL_CONTENT" file.txt
dfm add file.txt

# modifed by some VCS
write "$NEW_CONTENT" ./dotfiles/file.txt
assert_fail dfm add file.txt

assert "$NEW_CONTENT" = "$(cat ./dotfiles/file.txt)"
assert "$ORIGINAL_CONTENT" = "$(cat file.txt)"

dfm add -f file.txt
assert "$ORIGINAL_CONTENT" = "$(cat ./dotfiles/file.txt)"

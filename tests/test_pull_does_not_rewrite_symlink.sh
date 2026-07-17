OLD_CONTENT="$(uuid)"
NEW_CONTENT="$(uuid)"

dfm init dotfiles
write "$OLD_CONTENT" $PWD/dotfiles/file.txt
dfm pull -s
write "$NEW_CONTENT" $PWD/dotfiles/file.txt
assert "$NEW_CONTENT" = "$(cat file.txt)"
dfm pull file.txt
assert -L file.txt
assert "$NEW_CONTENT" = "$(cat file.txt)"

CONTENT="$(uuid)"
dfm init dotfiles
write "$CONTENT" file.txt
dfm add file.txt
rm file.txt
dfm pull -s
# assert that file.txt is a symlink
assert -L file.txt
assert "$CONTENT" = "$(cat file.txt)"

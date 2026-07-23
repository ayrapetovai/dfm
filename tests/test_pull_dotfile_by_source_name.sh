CONTENT="$(uuid)"

dfm init dotfiles
write "$CONTENT" .file
dfm add .file
rm .file
dfm pull .file
assert -f .file
assert_content_eq ".file" "$CONTENT"

CONTENT="$(uuid)"

dfm init dotfiles
write "$CONTENT" .file
dfm add .file
rm .file
dfm pull .file
assert -f .file
assert "$CONTENT" = "$(cat .file)"

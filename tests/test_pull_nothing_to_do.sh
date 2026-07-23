dfm init dotfiles

# nothing in source yet → pull should succeed with "nothing to do"
dfm pull

# add a file, pull it, then pull again → should succeed with nothing to do
CONTENT="$(uuid)"
write "$CONTENT" file.txt
dfm add file.txt
rm file.txt
dfm pull
assert -f file.txt
assert_content_eq "file.txt" "$CONTENT"

# everything is now in sync; another pull should be a no-op
dfm pull
assert -f file.txt
assert_content_eq "file.txt" "$CONTENT"

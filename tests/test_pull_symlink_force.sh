CONTENT="$(uuid)"

dfm init dotfiles
write "$CONTENT" file.txt
dfm add file.txt
rm file.txt
dfm pull -s file.txt
assert -L file.txt

# pull with --force should skip the "valid symlink" error and continue
dfm pull --force file.txt

# postcondition: still a symlink pointing to the source
assert -L file.txt
assert "$CONTENT" = "$(cat file.txt)"

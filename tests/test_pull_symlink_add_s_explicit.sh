CONTENT="$(uuid)"

dfm init dotfiles
write "$CONTENT" file.txt
dfm add -s file.txt

# precondition: target is a managed symlink into the source dir
assert -L file.txt
assert "$PWD/dotfiles/file.txt" = "$(readlink -f file.txt)"

# explicit target path pull on an already-managed symlink must succeed
assert_succ dfm pull file.txt

# postconditions: symlink preserved, still pointing at the source file
assert -L file.txt
assert "$PWD/dotfiles/file.txt" = "$(readlink -f file.txt)"
assert_content_eq "file.txt" "$CONTENT"

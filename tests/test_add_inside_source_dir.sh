CONTENT="$(uuid)"

dfm init dotfiles

# create a file inside the source directory
write "$CONTENT" "$PWD/dotfiles/some_file.txt"

# add by path to a file that's already in source dir → should skip
dfm add "$PWD/dotfiles/some_file.txt"

# postcondition: no additional source file was created (the original still exists)
assert_content_eq "$PWD/dotfiles/some_file.txt" "$CONTENT"

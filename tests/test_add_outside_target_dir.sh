CONTENT="$(uuid)"

dfm init dotfiles

# create a file outside the target directory ($PWD = target dir)
mkdir "$PWD/../other"
write "$CONTENT" "$PWD/../other/file.txt"

# add by path should skip files outside target directory
dfm add "$PWD/../other/file.txt"

# postcondition: no source file was created
assert_no_source "other/file.txt"

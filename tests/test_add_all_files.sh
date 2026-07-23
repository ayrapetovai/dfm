CONTENT1="$(uuid)"
CONTENT2="$(uuid)"

dfm init dotfiles

write "$CONTENT1" file1.txt
write "$CONTENT2" file2.txt

# add all untracked files in the target directory (no paths argument)
dfm add

# postcondition: both files were copied to source
assert_source "file1.txt"
assert_source "file2.txt"
assert_content_eq "$PWD/dotfiles/file1.txt" "$CONTENT1"
assert_content_eq "$PWD/dotfiles/file2.txt" "$CONTENT2"

CONTENT1="$(uuid)"
CONTENT2="$(uuid)"

dfm init dotfiles

mkdir dir
write "$CONTENT1" dir/file1.txt
write "$CONTENT2" dir/file2.txt

dfm add dir

assert -d "$PWD/dotfiles/dir"
assert_source "dir/file1.txt"
assert_source "dir/file2.txt"

assert_content_eq "$PWD/dotfiles/dir/file1.txt" "$CONTENT1"
assert_content_eq "$PWD/dotfiles/dir/file2.txt" "$CONTENT2"

CONTENT="$(uuid)"
dfm init dotfiles
mkdir -p "$PWD/dir1/dir2"
write "$CONTENT" "$PWD/dir1/dir2/file.txt"
dfm add dir1

assert_source "dir1/dir2/file.txt"
rm -rf dir1

dfm pull dir1
assert -f "$PWD/dir1/dir2/file.txt"
assert_content_eq "$PWD/dir1/dir2/file.txt" "$CONTENT"

rm -rf dir1
dfm pull dir1/dir2
assert -f "$PWD/dir1/dir2/file.txt"
assert_content_eq "$PWD/dir1/dir2/file.txt" "$CONTENT"

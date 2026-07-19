CONTENT1="$(uuid)"
CONTENT2="$(uuid)"

dfm init dotfiles

mkdir dir
write "$CONTENT1" dir/file1.txt
write "$CONTENT2" dir/file2.txt

dfm add dir

assert -d "$PWD/dotfiles/dir"
assert -f "$PWD/dotfiles/dir/file1.txt"
assert -f "$PWD/dotfiles/dir/file2.txt"

assert "$CONTENT1" = "$(cat $PWD/dotfiles/dir/file1.txt)"
assert "$CONTENT2" = "$(cat $PWD/dotfiles/dir/file2.txt)"

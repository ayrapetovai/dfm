# sync on fully-synchronized files is a no-op (succeeds, changes nothing).

dfm init dotfiles
write "content1" file1.txt
write "content2" file2.txt
write "content3" file3.txt
dfm add file1.txt
dfm add file2.txt
rm file2.txt

# Both sides agree and are synced: sync must succeed and change nothing.
dfm sync

assert_content_eq "file1.txt" "content1"
assert_content_eq "$PWD/dotfiles/file1.txt" "content1"

assert_fail test -f file2.txt
assert_content_eq "$PWD/dotfiles/file2.txt" "content2"

assert -f file3.txt
assert_no_source "file3.txt"
assert_content_eq "$PWD/file3.txt" "content3"

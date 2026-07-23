# A2 — forget without paths traverses the target directory
dfm init dotfiles

write "content1" file1.txt
write "content2" file2.txt
dfm add file1.txt
dfm add file2.txt

# forget without paths — traverses entire target dir, finds managed files
dfm forget

# source files must be removed
assert_fail test -f "$PWD/dotfiles/file1.txt"
assert_fail test -f "$PWD/dotfiles/file2.txt"

# target files must still exist
assert -f file1.txt
assert -f file2.txt
assert "content1" = "$(cat file1.txt)"
assert "content2" = "$(cat file2.txt)"

# pull must not recreate them (state is also cleaned up)
rm -f file1.txt file2.txt
dfm pull
assert_fail test -f file1.txt
assert_fail test -f file2.txt

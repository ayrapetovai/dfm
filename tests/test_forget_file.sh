dfm init dotfiles
write $(uuid) file.txt
dfm add file.txt
dfm forget file.txt
rm file.txt
dfm pull
assert_fail test -f file.txt

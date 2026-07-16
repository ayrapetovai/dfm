dfm init dotfiles
write $(uuid) file.txt
dfm add file.txt
rm file.txt
dfm pull -s file.txt
assert -L file.txt
assert_fail dfm pull file.txt
assert -L file.txt

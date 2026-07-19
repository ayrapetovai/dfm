dfm init dotfiles
write "content" file.txt
dfm add file.txt
rm file.txt
dfm pull -s file.txt
assert -L file.txt
assert_fail dfm add -e file.txt

write "content" other_file.txt
assert_fail dfm add -es other_file.txt

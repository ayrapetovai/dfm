dfm init dotfiles
write "content" file.txt
dfm ignore file.txt
dfm add file.txt
assert_fail test -f dotfiles/file.txt

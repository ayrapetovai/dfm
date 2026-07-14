dfm init dotfiles
write "content1" file.txt
dfm add file.txt
write "content2" file.txt
dfm add file.txt
rm file.txt
dfm pull
assert -f file.txt
assert "content2" = "$(cat file.txt)"

dfm  init "dotfiles"
write "content1" file.txt
dfm add file.txt
rm file.txt
dfm pull
assert "content1" = "$(cat file.txt)"

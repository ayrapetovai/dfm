dfm init dotfiles
touch file.txt
chmod a+x file.txt
assert "775" = "$(stat -c '%a' file.txt)"
dfm add file.txt
assert "775" = "$(stat -c '%a' $PWD/dotfiles/file.txt)"
rm file.txt
dfm pull file.txt
assert "775" = "$(stat -c '%a' file.txt)"

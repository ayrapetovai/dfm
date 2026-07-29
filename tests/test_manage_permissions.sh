dfm init dotfiles
touch file.txt
chmod 0755 file.txt
assert "755" = "$(stat -c '%a' file.txt)"
dfm add file.txt
assert "755" = "$(stat -c '%a' $PWD/dotfiles/file.txt)"
rm file.txt
dfm pull file.txt
assert "755" = "$(stat -c '%a' file.txt)"

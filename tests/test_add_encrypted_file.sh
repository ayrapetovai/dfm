CONTENT="$(uuid)"
dfm init dotfiles
write "$CONTENT" file.txt
CONFIG=$(dfm paths | grep config | sed 's/^config[[:space:]]*"\(.*\)"$/\1/')
cat "$CONFIG" | sed 's/obtain_password_shell_command.*$/obtain_password_shell_command = "echo -n 1234"/g' > ./newconfig
mv ./newconfig "$CONFIG"
dfm add -e file.txt
rm file.txt
7z -p1234 x "$PWD/dotfiles/file.txt.encrypted"
assert "$CONTENT" = "$(cat file.txt)"

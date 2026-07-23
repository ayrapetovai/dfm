CONTENT="$(uuid)"
PASSWORD="$(uuid)"

dfm init dotfiles
write "$CONTENT" file.txt
dfm config --set obtain_password_shell_command "echo -n $PASSWORD"
dfm add -e file.txt
rm file.txt
7z -p"$PASSWORD" x "$PWD/dotfiles/file.txt.encrypted"
assert_content_eq "file.txt" "$CONTENT"

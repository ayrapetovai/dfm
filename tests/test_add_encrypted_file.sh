CONTENT="$(uuid)"
PASSWORD="$(uuid)"

dfm init dotfiles
write "$CONTENT" file.txt
dfm config --set obtain_password_shell_command "echo -n $PASSWORD"
dfm add -e file.txt
assert_encrypted "file.txt" "$CONTENT"

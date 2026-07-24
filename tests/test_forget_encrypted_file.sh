# dfm forget must remove the encrypted source file when the target was
# added with --encrypt.

PASSWORD="$(uuid)"
CONTENT="$(uuid)"

dfm init dotfiles
dfm config --set obtain_password_shell_command "echo -n $PASSWORD"

# add an encrypted file
write "$CONTENT" secret.txt
dfm add --encrypt secret.txt
assert_source "secret.txt.encrypted"
assert_no_source "secret.txt"

# forget it — must remove the encrypted source
dfm forget secret.txt

# encrypted source is gone
assert_no_source "secret.txt.encrypted"

# target file still exists
assert_content_eq "secret.txt" "$CONTENT"

# pull must have nothing to do (source is gone)
dfm pull
assert_no_source "secret.txt.encrypted"

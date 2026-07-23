# Subsequent `dfm pull` should skip encrypted files when neither the
# target nor the encrypted source was modified.

PASSWORD="$(uuid)"
CONTENT="$(uuid)"

dfm init dotfiles
dfm config --set obtain_password_shell_command "echo -n $PASSWORD"

write "$CONTENT" secret.txt
dfm add --encrypt secret.txt

# first pull — creates target from encrypted source
rm secret.txt
dfm pull
assert_content_eq "secret.txt" "$CONTENT"

# second pull — nothing changed, should skip silently
dfm pull
assert_content_eq "secret.txt" "$CONTENT"

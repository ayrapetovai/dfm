# Standalone `dfm encrypt` / `dfm decrypt` round-trip and wrong-password
# detection. The password provider always returns the same PASSWORD, so after
# an (intentional) bad attempt the cache is cleared and the decrypt succeeds.

PASSWORD="$(uuid)"
CONTENT="$(uuid)"

dfm init dotfiles
dfm config --set obtain_password_shell_command "bash -c 'cat \$0; echo' \"$PASSWORD\""

write "$CONTENT" secret.txt
dfm encrypt secret.txt -o secret.txt.encrypted

assert -f secret.txt.encrypted
assert_content_eq "secret.txt" "$CONTENT"

dfm decrypt secret.txt.encrypted -o restored.txt
assert_content_eq "restored.txt" "$CONTENT"


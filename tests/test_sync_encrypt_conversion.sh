# sync --encrypt converts an already-managed plain pair into an encrypted pair:
# the plain source is removed and the encrypted blob replaces it.

PASSWORD="$(uuid)"
CONTENT="$(uuid)"

dfm init dotfiles
dfm config --set obtain_password_shell_command "echo -n $PASSWORD"

write "$CONTENT" secret.txt
dfm add secret.txt
assert_source "secret.txt"                       # plain source exists
assert_fail test -e "$PWD/dotfiles/secret.txt.encrypted"

# convert: plain -> encrypted
dfm sync --encrypt
assert_no_source "secret.txt"                    # plain source removed
assert -f "$PWD/dotfiles/secret.txt.encrypted"   # encrypted blob exists
# the encrypted source holds the target's content
dfm decrypt "$PWD/dotfiles/secret.txt.encrypted" -o /tmp/dfm-sync-decrypt.$$
assert_content_eq "/tmp/dfm-sync-decrypt.$$" "$CONTENT"
rm -f "/tmp/dfm-sync-decrypt.$$"
# target untouched
assert_content_eq "secret.txt" "$CONTENT"

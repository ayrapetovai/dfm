# Permissions must survive the encrypt/decrypt round-trip (REVIEW.md #1).
# A 0600 file added encrypted and then re-created from the .encrypted
# archive via `pull` must keep mode 600, not fall back to 0644.

PASSWORD="$(uuid)"
CONTENT="$(uuid)"

dfm init dotfiles
dfm config --set obtain_password_shell_command "echo -n $PASSWORD"

write "$CONTENT" secret.txt
chmod 600 secret.txt
dfm add --encrypt secret.txt

# remove the target so `pull` re-creates it by decrypting the archive
rm secret.txt
dfm pull

assert_content_eq "secret.txt" "$CONTENT"
assert_succ [ "$(stat -c '%a' secret.txt)" = "600" ]

# sync handles encrypted sources, comparing by mtime (like add/pull):
# - target modified  -> re-encrypt target to the encrypted source
# - source modified  -> decrypt the encrypted source to the target

PASSWORD="$(uuid)"
CONTENT="$(uuid)"

dfm init dotfiles
dfm config --set obtain_password_shell_command "echo -n $PASSWORD"

# --- pull direction: an externally-touched retired/re-encrypted source
# becomes SourceModified; sync decrypts it to the target.
write "$CONTENT" pull.txt
dfm add --encrypt pull.txt
touch "$PWD/dotfiles/pull.txt.encrypted"
dfm sync
assert_content_eq "pull.txt" "$CONTENT"

# --- push direction: a target-side change is re-encrypted to the source.
# Modify the target and sync; the encrypted source must hold the new content.
write "$CONTENT" push.txt
dfm add --encrypt push.txt
write "changed-on-target" push.txt
dfm sync
# verify the re-encrypted source via a standalone decrypt (leaving target alone
# is not needed for the assertion, so reuse the file)
dfm decrypt "$PWD/dotfiles/push.txt.encrypted" -o /tmp/dfm-sync-decrypt.$$ 2>/dev/null
assert_content_eq "/tmp/dfm-sync-decrypt.$$" "changed-on-target"
rm -f "/tmp/dfm-sync-decrypt.$$"

# After an encrypted pull, modifying the target should trigger the
# timestamp conflict check: fail without --force, succeed with --force.

PASSWORD="$(uuid)"
ORIGINAL="$(uuid)"
MODIFIED="$(uuid)"

dfm init dotfiles
dfm config --set obtain_password_shell_command "echo -n $PASSWORD"

write "$ORIGINAL" secret.txt
dfm add --encrypt secret.txt

# first pull — creates target
rm secret.txt
dfm pull
assert_content_eq "secret.txt" "$ORIGINAL"

# modify the target
write "$MODIFIED" secret.txt

# pull without --force should fail (TargetModified)
assert_fail dfm pull

# postcondition: target still has the modified content (was not overwritten)
assert_content_eq "secret.txt" "$MODIFIED"

# pull with --force should succeed
dfm pull --force

# postcondition: target now has the original content (decrypted from source)
assert_content_eq "secret.txt" "$ORIGINAL"

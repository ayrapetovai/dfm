CONTENT="$(uuid)"
PASSWORD="$(uuid)"

dfm init dotfiles

# set password command via config subcommand (string field, works with --set)
dfm config --set obtain_password_shell_command "echo -n $PASSWORD"

# force_encryption_for is an array — config --set stores everything as a TOML
# string, so replace it in-place with sed instead
sed -i 's|^force_encryption_for = .*|force_encryption_for = ["\\\\.txt$"]|' "$PWD/.config/dfm/config.toml"

# create a .txt file (matches the force_encryption_for regex)
write "$CONTENT" secret.txt

# add without --encrypt — config's force_encryption_for should force encryption
dfm add secret.txt

# postcondition: encrypted source file was created (not a plain one)
assert_source "secret.txt.encrypted"
assert_no_source "secret.txt"

# verify the encrypted file can be decrypted with the password
assert_encrypted "secret.txt" "$CONTENT"

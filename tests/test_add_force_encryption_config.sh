CONTENT="$(uuid)"
PASSWORD="$(uuid)"

dfm init dotfiles

# write config with force_encryption_for using echo to avoid heredoc escaping issues
echo 'dot_prefix = "dot_"' > "$PWD/.config/dfm/config.toml"
echo 'symlink_postfix = ".symlink"' >> "$PWD/.config/dfm/config.toml"
echo 'encrypted_postfix = ".encrypted"' >> "$PWD/.config/dfm/config.toml"
echo 'manage_symlinks = true' >> "$PWD/.config/dfm/config.toml"
echo 'dotfiles_only = false' >> "$PWD/.config/dfm/config.toml"
echo 'force_encryption_for = ["\\.txt$"]' >> "$PWD/.config/dfm/config.toml"
echo "obtain_password_shell_command = \"echo -n $PASSWORD\"" >> "$PWD/.config/dfm/config.toml"

# create a .txt file (matches the force_encryption_for regex)
write "$CONTENT" secret.txt

# add without --encrypt — config's force_encryption_for should force encryption
dfm add secret.txt

# postcondition: encrypted source file was created (not a plain one)
assert -f "$PWD/dotfiles/secret.txt.encrypted"
assert_fail test -f "$PWD/dotfiles/secret.txt"

# verify the encrypted file can be decrypted with the password
rm secret.txt
7z -p"$PASSWORD" x -y "$PWD/dotfiles/secret.txt.encrypted" > /dev/null 2>&1
assert "$CONTENT" = "$(cat secret.txt)"

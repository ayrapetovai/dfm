CONTENT="$(uuid)"
PASSWORD="$(uuid)"

dfm init dotfiles
dfm config --set obtain_password_shell_command "echo -n $PASSWORD"

# first add as plain file
write "$CONTENT" file.txt
dfm add file.txt
assert_source "file.txt"

# now add with --encrypt when an unencrypted source file already exists
dfm add -e file.txt

# postcondition: encrypted source file exists
assert_source "file.txt.encrypted"

# postcondition: plain source file was removed (no longer orphaned)
assert_no_source "file.txt"

# verify the encrypted file decrypts correctly
assert_encrypted "file.txt" "$CONTENT"

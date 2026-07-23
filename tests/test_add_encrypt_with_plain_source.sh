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

# postcondition: encrypted source file exists (plain source was replaced or supplemented)
assert_source "file.txt.encrypted"

# verify the encrypted file decrypts correctly
rm file.txt
7z -p"$PASSWORD" x -y "$PWD/dotfiles/file.txt.encrypted" > /dev/null 2>&1
assert_content_eq "file.txt" "$CONTENT"

CONTENT="$(uuid)"
MODIFIED="$(uuid)"
PASSWORD="$(uuid)"

dfm init dotfiles
dfm config --set obtain_password_shell_command "echo -n $PASSWORD"

# add as plain file
write "$CONTENT" file.txt
dfm add file.txt
assert_source "file.txt"

# modify the plain source
write "$MODIFIED" dotfiles/file.txt

# encrypt should reject due to SourceModified
assert_fail dfm add -e file.txt 2>/dev/null

# with --force it should succeed
dfm add -e --force file.txt

# encrypted source exists
assert_source "file.txt.encrypted"

# plain source was cleaned up
assert_no_source "file.txt"

# verify decrypted content matches target (add direction: target is truth)
rm file.txt
7z -p"$PASSWORD" x -y "$PWD/dotfiles/file.txt.encrypted" > /dev/null 2>&1
assert_content_eq "file.txt" "$CONTENT"

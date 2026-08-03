# A `dfm pull` of an encrypted file must recreate every enclosing directory
# with the permissions it had when the file was added. Regression test for the
# bug where decrypt's `create_dir_all` reset a restricted (e.g. 0700/0701)
# directory to the umask default (0755).

PASSWORD="$(uuid)"
CONTENT="$(uuid)"

dfm init dotfiles
dfm config --set obtain_password_shell_command "echo -n $PASSWORD"

# A nested file under a pair of directories with non-default permissions.
write "$CONTENT" private/sub/f.conf
chmod 700 private
chmod 701 private/sub

dfm add --encrypt private/sub/f.conf

assert_source "private/sub/f.conf.encrypted"
assert_no_source "private/sub/f.conf"

# Decrypt round-trips match (also proves the archive is well-formed).
assert_encrypted "private/sub/f.conf" "$CONTENT"

# Remove the whole directory tree from the target, then pull it back.
rm -rf private

dfm pull

# The file is restored with the right content...
assert_content_eq "private/sub/f.conf" "$CONTENT"

# ...and the enclosing directories keep their recorded permissions.
assert "$(stat -c '%a' private)" = "700"
assert "$(stat -c '%a' private/sub)" = "701"
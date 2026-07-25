# dfm merge with encrypted source files — the state key ends with .encrypted,
# source_rel_to_target_rel strips the postfix so the target path is correct,
# and run_merge decrypts before merging then re-encrypts the result.

PASSWORD="$(uuid)"
ORIGINAL="$(uuid)"
MODIFIED="$(uuid)"
SOURCE_MODIFIED="$(uuid)"

dfm init dotfiles
dfm config --set obtain_password_shell_command "echo -n $PASSWORD"
dfm config --set merge_tool_command "cp {target} {result}"

# Setup: create a clean encrypted state, then pull to target
write "$ORIGINAL" secret.txt
dfm add --encrypt secret.txt
assert_encrypted "secret.txt" "$ORIGINAL"

rm secret.txt
dfm pull
assert_content_eq "secret.txt" "$ORIGINAL"

# modify the target
write "$MODIFIED" secret.txt

# modify the encrypted source by re-encrypting different content under a
# different name, then replacing the original encrypted source file
write "$SOURCE_MODIFIED" v2.txt
dfm add --encrypt --force v2.txt
mv "$PWD/dotfiles/v2.txt.encrypted" "$PWD/dotfiles/secret.txt.encrypted"
rm -f v2.txt

# now the state still has the original sync_time for "secret.txt.encrypted"
# but the source file has new content and target has new content → BothModified

# Act: no-args branch iterates state, strips .encrypted from target path
dfm merge

# Assert: merged content (copied from target via merge tool) appears on both sides
assert_content_eq "secret.txt" "$MODIFIED"

# Re-pull to verify the encrypted source was also updated with merged content
rm secret.txt
dfm pull
assert_content_eq "secret.txt" "$MODIFIED"

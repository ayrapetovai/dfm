# dfm merge PATH where the state key has .encrypted postfix.
# The target-path branch uses resolve_state_key to find the encrypted variant.

PASSWORD="$(uuid)"
ORIGINAL="$(uuid)"
MODIFIED="$(uuid)"
SOURCE_MODIFIED="$(uuid)"

dfm init dotfiles
dfm config --set obtain_password_shell_command "echo -n $PASSWORD"
dfm config --set merge_tool_command "cp {target} {result}"

# ------------------------------------------------------------------
# Setup: same as test_merge_encrypted.sh
# ------------------------------------------------------------------
write "$ORIGINAL" secret.txt
dfm add --encrypt secret.txt
assert_encrypted "secret.txt" "$ORIGINAL"

rm secret.txt
dfm pull
assert_content_eq "secret.txt" "$ORIGINAL"

write "$MODIFIED" secret.txt

write "$SOURCE_MODIFIED" v2.txt
dfm add --encrypt --force v2.txt
mv "$PWD/dotfiles/v2.txt.encrypted" "$PWD/dotfiles/secret.txt.encrypted"
rm -f v2.txt

# ------------------------------------------------------------------
# Act: target-path branch — resolve_state_key tries .encrypted variant
# ------------------------------------------------------------------
dfm merge secret.txt

# ------------------------------------------------------------------
# Assert
# ------------------------------------------------------------------
assert_content_eq "secret.txt" "$MODIFIED"

# re-pull to verify encrypted source was re-encrypted with merged content
rm secret.txt
dfm pull
assert_content_eq "secret.txt" "$MODIFIED"

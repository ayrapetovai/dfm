# dfm merge PATH where the state key has .encrypted postfix.
# Covers ONLY the source-path branch: the state key is resolved with the
# .encrypted postfix no matter whether the user passes the plain name or the
# postfixed one. (Issue: "dfm merge filepath does not merge files" for
# encrypted sources was fixed by resolving the postfix variants here.)

PASSWORD="$(uuid)"
ORIGINAL="$(uuid)"
MODIFIED="$(uuid)"
SOURCE_MODIFIED="$(uuid)"

dfm init dotfiles
dfm config --set obtain_password_shell_command "echo -n $PASSWORD"
dfm config --set merge_tool_command "cp {target} {result}"

# Setup: a managed encrypted file with both sides modified (same as
# test_merge_by_path_encrypted.sh)
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

# Act 1: run from the source directory and pass the plain (postfix-less) name.
# The relative PATH is anchored at CWD, so it resolves inside the source dir.
HOME_DIR="$PWD"
cd "$HOME_DIR/dotfiles"
dfm merge secret.txt
cd "$HOME_DIR"

# Assert: merge ran and kept the target version
assert_content_eq "secret.txt" "$MODIFIED"

# re-pull to verify the encrypted source was re-encrypted with merged content
rm secret.txt
dfm pull
assert_content_eq "secret.txt" "$MODIFIED"

# Act 2: divergent again, this time passing the postfixed source path
write "$MODIFIED" secret.txt
write "$SOURCE_MODIFIED" v3.txt
dfm add --encrypt --force v3.txt
mv "$PWD/dotfiles/v3.txt.encrypted" "$PWD/dotfiles/secret.txt.encrypted"
rm -f v3.txt

dfm merge "$PWD/dotfiles/secret.txt.encrypted"

assert_content_eq "secret.txt" "$MODIFIED"

rm secret.txt
dfm pull
assert_content_eq "secret.txt" "$MODIFIED"
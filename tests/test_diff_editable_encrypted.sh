# `dfm diff --editable` on an encrypted source decrypts the blob into its
# scratch copy and re-encrypts it on write-back, but only when the source side
# was actually edited: each side is saved to its own file, and the plaintext
# never lands in the source directory.

PASSWORD="$(uuid)"
SECRET="$(uuid)"
EDITED_TARGET="$(uuid)"
EDITED_SOURCE="$(uuid)"

dfm init dotfiles
dfm config --set obtain_password_shell_command "echo -n $PASSWORD"

write "$SECRET" secret.txt
dfm add --encrypt secret.txt
assert_source "secret.txt.encrypted"
assert_encrypted "secret.txt" "$SECRET"

# Editing only the {target} copy leaves the encrypted blob untouched. The blob
# is checked by decrypting into a separate file: assert_encrypted would
# overwrite the edited target with the decrypted plaintext.
write "$EDITED_TARGET" edit.txt
dfm config --set diff_editable_tool_command "cp $HOME/edit.txt {target}"
dfm diff --editable secret.txt
assert_content_eq "secret.txt" "$EDITED_TARGET"
dfm decrypt "$PWD/dotfiles/secret.txt.encrypted" -o decrypted-check.txt
assert_content_eq "decrypted-check.txt" "$SECRET"
assert_source "secret.txt.encrypted"
assert_no_source "secret.txt"
assert ! -e "$PWD/dotfiles/.current_diff"

# Editing the {source} copy on the now-differing pair re-encrypts the blob with
# the edited plaintext, keeping the metadata the target file provides.
write "$EDITED_SOURCE" edit.txt
dfm config --set diff_editable_tool_command "cp $HOME/edit.txt {source}"
dfm diff --editable secret.txt
assert_content_eq "secret.txt" "$EDITED_TARGET"
dfm decrypt "$PWD/dotfiles/secret.txt.encrypted" -o decrypted-check2.txt
assert_content_eq "decrypted-check2.txt" "$EDITED_SOURCE"
assert_source "secret.txt.encrypted"
assert_no_source "secret.txt"
assert ! -e "$PWD/dotfiles/.current_diff"

# The differing sides must NOT be recorded as synchronized: the read-only mode
# still treats the pair as diverged and runs the diff tool instead of printing
# "is synchronized".
dfm config --set diff_tool_command "true"
diff_out="$(dfm diff secret.txt 2>/dev/null)"
assert_fail grep -qF "secret.txt is synchronized" <<<"$diff_out"


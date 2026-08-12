# `dfm diff` on an encrypted source shows a diff of the *decrypted* content:
# the configured diff tool receives the plaintext (both as a temp file for
# {source} and piped to its stdin), never the .encrypted bytes. An encrypted
# file that is not modified reports "is synchronized" without prompting.

PASSWORD="$(uuid)"
SECRET="$(uuid)"
MODIFIED="$(uuid)"

dfm init dotfiles
dfm config --set obtain_password_shell_command "echo -n $PASSWORD"
dfm config --set diff_tool_command "cp {source} $HOME/decrypted.txt"

write "$SECRET" secret.txt
dfm add --encrypt secret.txt
assert_source "secret.txt.encrypted"

# unchanged → synchronized
assert_succ grep -qF "secret.txt is synchronized" <<<$(dfm diff secret.txt 2>/dev/null)

# target modified → the diff tool receives the decrypted source
write "$MODIFIED" secret.txt
dfm diff secret.txt >/dev/null 2>&1
assert_content_eq "$HOME/decrypted.txt" "$SECRET"

# the target file itself is untouched by the diff
assert_content_eq "secret.txt" "$MODIFIED"

# a `.current_diff` temp dir must not be left behind
assert ! -e "$PWD/dotfiles/.current_diff"

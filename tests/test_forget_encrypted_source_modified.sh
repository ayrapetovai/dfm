# dfm forget must check modification clashes for encrypted source files,
# just like it does for plain sources.

PASSWORD="$(uuid)"
ORIGINAL="$(uuid)"
MODIFIED="$(uuid)"

dfm init dotfiles
dfm config --set obtain_password_shell_command "echo -n $PASSWORD"

# ------------------------------------------------------------------
# 1. SourceModified: touch encrypted source, forget without --force → fail
# ------------------------------------------------------------------
write "$ORIGINAL" secret.txt
dfm add --encrypt secret.txt
assert_source "secret.txt.encrypted"

touch "$PWD/dotfiles/secret.txt.encrypted"

assert_fail dfm forget secret.txt 2>/dev/null

# postcondition: encrypted source still exists
assert_source "secret.txt.encrypted"

# forget with --force must succeed
dfm forget --force secret.txt
assert_no_source "secret.txt.encrypted"

# re-add for the next scenario
write "$ORIGINAL" secret.txt
dfm add --encrypt secret.txt

# ------------------------------------------------------------------
# 2. BothModified: touch encrypted source AND modify target
# ------------------------------------------------------------------
write "$MODIFIED" secret.txt
touch "$PWD/dotfiles/secret.txt.encrypted"

assert_fail dfm forget secret.txt 2>/dev/null

# still exists
assert_source "secret.txt.encrypted"
assert_content_eq "secret.txt" "$MODIFIED"

# with --force it must go
dfm forget --force secret.txt
assert_no_source "secret.txt.encrypted"
assert_content_eq "secret.txt" "$MODIFIED"

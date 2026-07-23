# dfm add --encrypt must perform the same timestamp conflict checks as
# regular add. Scenarios:
#
# 1. NonModified    — add --encrypt twice with no changes → clean skip
# 2. SourceModified — externally-touched encrypted source, add without --force → fail
# 3. SourceModified with --force → succeed and re-encrypt

PASSWORD="$(uuid)"
ORIGINAL="$(uuid)"
MODIFIED="$(uuid)"

dfm init dotfiles
dfm config --set obtain_password_shell_command "echo -n $PASSWORD"

# ------------------------------------------------------------------
# 1. NonModified: second add --encrypt with nothing changed
# ------------------------------------------------------------------
write "$ORIGINAL" secret.txt
dfm add --encrypt secret.txt

# second add with no changes — must succeed without --force
# (this relies on the state being recorded after the first add — our fix)
dfm add --encrypt secret.txt

# verify the encrypted source still decrypts correctly
rm secret.txt
7z -p"$PASSWORD" x "$PWD/dotfiles/secret.txt.encrypted" > /dev/null 2>&1
assert_content_eq "secret.txt" "$ORIGINAL"

# ------------------------------------------------------------------
# 2. SourceModified: touch encrypted source, add without --force → fail
# ------------------------------------------------------------------
touch "$PWD/dotfiles/secret.txt.encrypted"

assert_fail dfm add --encrypt secret.txt

# postcondition: encrypted source unchanged
rm secret.txt
7z -p"$PASSWORD" x "$PWD/dotfiles/secret.txt.encrypted" > /dev/null 2>&1
assert_content_eq "secret.txt" "$ORIGINAL"

# ------------------------------------------------------------------
# 3. SourceModified with --force → succeed and re-encrypt
# ------------------------------------------------------------------
write "$MODIFIED" secret.txt

# touch the encrypted source again so mtime > new sync time isn't an issue
touch "$PWD/dotfiles/secret.txt.encrypted"

dfm add --encrypt --force secret.txt

# postcondition: encrypted source now has the modified content
rm secret.txt
7z -p"$PASSWORD" x "$PWD/dotfiles/secret.txt.encrypted" > /dev/null 2>&1
assert_content_eq "secret.txt" "$MODIFIED"

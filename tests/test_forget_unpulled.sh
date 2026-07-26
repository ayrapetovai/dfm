# Point 9: forget must succeed when the target is absent, regardless of
# modification state or encryption.
#
# Scenarios:
#   1. Encrypted source, target absent, source clean     → forget succeeds
#   2. Encrypted source, target absent, source modified  → forget succeeds (no --force)
#   3. Symlink pointer source, target absent             → forget succeeds
#   4. Plain source, target absent, source modified      → forget succeeds (no --force)

PASSWORD="$(uuid)"
ORIGINAL="$(uuid)"
MODIFIED="$(uuid)"

dfm init dotfiles
dfm config --set obtain_password_shell_command "echo -n $PASSWORD"

# 1. Encrypted source, target absent, source clean
write "$ORIGINAL" secret.txt
dfm add --encrypt secret.txt
assert_source "secret.txt.encrypted"
assert_no_source "secret.txt"

# remove target — simulate "never pulled"
rm secret.txt

dfm forget secret.txt
assert_no_source "secret.txt.encrypted"

# 2. Encrypted source, target absent, source modified
write "$ORIGINAL" secret.txt
dfm add --encrypt secret.txt
assert_source "secret.txt.encrypted"

# remove target AND modify the encrypted source
rm secret.txt
touch "$PWD/dotfiles/secret.txt.encrypted"

# must succeed without --force (target is absent)
dfm forget secret.txt
assert_no_source "secret.txt.encrypted"

# 3. Symlink pointer source, target absent
mkdir -p real_files
echo "real content" > "real_files/other.txt"
ln -s "real_files/other.txt" "mylink"

dfm add mylink
assert_source "mylink.symlink"

# remove target symlink — simulate "never pulled"
rm mylink

dfm forget mylink
assert_no_source "mylink.symlink"

# 4. Plain source, target absent, source modified
CONTENT="$(uuid)"
write "$CONTENT" plain.txt
dfm add plain.txt
assert_source "plain.txt"

# remove target AND modify the plain source
rm plain.txt
touch "$PWD/dotfiles/plain.txt"

# must succeed without --force (target is absent)
dfm forget plain.txt
assert_no_source "plain.txt"

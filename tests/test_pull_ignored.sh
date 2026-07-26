CONTENT="$(uuid)"
NEW_CONTENT="$(uuid)"
dfm init dotfiles

# --- setup: add a file and ignore it ---
write "$CONTENT" file.txt
dfm add file.txt
assert_source file.txt
dfm ignore file.txt

write "$NEW_CONTENT" "$PWD/dotfiles/file.txt"

dfm pull
assert_content_eq "file.txt" "$CONTENT"

dfm pull file.txt
assert_content_eq "file.txt" "$CONTENT"

# --- case 1: pull without args (traverses source dir) ---
# The ignored file should NOT be restored
rm -f file.txt
dfm pull
assert_fail test -f file.txt

# --- case 2: pull by explicit target path ---
# The ignored file should NOT be restored
dfm pull file.txt
assert_fail test -f file.txt

# --- case 3: pull by source dir explicitly ---
# The ignored file should NOT be restored
dfm pull "$HOME/dotfiles"
assert_fail test -f file.txt

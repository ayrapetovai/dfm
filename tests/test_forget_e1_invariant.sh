# E1 — design invariant: forget must never delete a managed regular file
# in the target directory.

dfm init dotfiles

# --- scenario 1: basic forget of an unmodified file ---
CONTENT1="$(uuid)"
write "$CONTENT1" f1.txt
dfm add f1.txt
dfm forget f1.txt
assert -f f1.txt
assert_content_eq "f1.txt" "$CONTENT1"

# --- scenario 2: forget with target modified (no --force fails, target stays) ---
CONTENT2="$(uuid)"
write "$CONTENT2" f2.txt
dfm add f2.txt
write "modified-$CONTENT2" f2.txt
assert_fail dfm forget f2.txt
assert -f f2.txt
assert_content_eq "f2.txt" "modified-$CONTENT2"

# --- scenario 3: forget with target modified and --force ---
dfm forget --force f2.txt
assert -f f2.txt
assert_content_eq "f2.txt" "modified-$CONTENT2"

# --- scenario 4: forget with both modified and --force ---
CONTENT3="$(uuid)"
write "$CONTENT3" f3.txt
dfm add f3.txt
write "modified-target-$CONTENT3" f3.txt
write "modified-source-$CONTENT3" "$PWD/dotfiles/f3.txt"
dfm forget --force f3.txt
assert -f f3.txt
assert_content_eq "f3.txt" "modified-target-$CONTENT3"

# --- scenario 5: forget with source modified and --force ---
CONTENT4="$(uuid)"
write "$CONTENT4" f4.txt
dfm add f4.txt
write "modified-source-$CONTENT4" "$PWD/dotfiles/f4.txt"
dfm forget --force f4.txt
assert -f f4.txt
assert_content_eq "f4.txt" "$CONTENT4"

# --- scenario 6: forget when source doesn't exist ---
CONTENT5="$(uuid)"
write "$CONTENT5" f5.txt
dfm add f5.txt
rm "$PWD/dotfiles/f5.txt"
dfm forget f5.txt
assert -f f5.txt
assert_content_eq "f5.txt" "$CONTENT5"

# --- scenario 7: forget without paths (traverses target dir) ---
CONTENT6="$(uuid)"
CONTENT7="$(uuid)"
write "$CONTENT6" f6.txt
write "$CONTENT7" f7.txt
dfm add f6.txt
dfm add f7.txt
dfm forget
assert -f f6.txt
assert_content_eq "f6.txt" "$CONTENT6"
assert -f f7.txt
assert_content_eq "f7.txt" "$CONTENT7"

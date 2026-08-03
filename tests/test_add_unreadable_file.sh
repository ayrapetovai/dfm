# A file with restrictive permissions (mode 000) must be skipped with a
# warning instead of aborting `dfm add`. Other files are still added, and a
# normal add works once the permissions are restored.

dfm init dotfiles

# add a single unreadable file → succeeds with a warning, no source artifact
write "secret" "locked.txt"
chmod 000 "locked.txt"

dfm add locked.txt 2>&1 | grep -q "skipping unreadable path"
assert_no_source "locked.txt"

# an unreadable file among readable ones must not abort the add
write "plain" "plain.txt"
dfm add . 2>&1 | grep -q "skipping unreadable path"
assert_source "plain.txt"
assert_no_source "locked.txt"

# restore permissions → the file is added normally
chmod 644 "locked.txt"
dfm add locked.txt
assert_source "locked.txt"

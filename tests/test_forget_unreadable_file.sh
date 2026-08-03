# `dfm forget` must not abort when the target file is unreadable — it skips
# the file (cannot verify content), and forgets it normally once permissions
# are restored.

dfm init dotfiles
write "content" "file.txt"
dfm add file.txt
assert_source "file.txt"

chmod 000 file.txt

# unreadable target → forget warns and skips, source artifact stays
dfm forget file.txt 2>&1 | grep -q "skipping unreadable path"
assert_source "file.txt"

# restore permissions → the file is forgotten normally
chmod 644 file.txt
dfm forget file.txt
assert_no_source "file.txt"
dfm status --short | grep -q "?? file.txt"

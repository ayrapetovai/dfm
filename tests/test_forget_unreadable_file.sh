# `dfm forget` must not abort when the target file is unreadable — it skips
# the file (cannot verify content), and forgets it normally once permissions
# are restored.

dfm init dotfiles
write "content" "file.txt"
dfm add file.txt
assert_source "file.txt"

chmod 000 file.txt

# unreadable target → forget warns and skips, source artifact stays
# forget succeeds (exit 0) AND warns on stderr
dfm forget file.txt >/dev/null 2>warn.txt
assert_succ grep -qF "skipping unreadable path" warn.txt
assert_source "file.txt"

# restore permissions → the file is forgotten normally
chmod 644 file.txt
dfm forget file.txt
assert_no_source "file.txt"
dfm status --short | grep -q "?? file.txt"

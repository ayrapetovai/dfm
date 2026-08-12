# `dfm diff` on an ignored file prints "<path> is ignored by <regexp>",
# where <regexp> is the pattern stored in the ignore file.

dfm init dotfiles

dfm ignore secret.txt >/dev/null 2>&1
write "data" secret.txt

assert_succ grep -qF "is ignored by" <<<$(dfm diff secret.txt 2>/dev/null)

# the exact pattern from the ignore file is echoed back
dfm diff secret.txt >out.txt 2>/dev/null
assert_succ grep -qF "secret.txt is ignored by secret\\.txt" out.txt

# a directory ignore prunes the whole subtree from diff as well
dfm ignore somedir >/dev/null 2>&1
write "nested" somedir/inner.txt
assert_succ grep -qF "is ignored by" <<<$(dfm diff somedir/inner.txt 2>/dev/null)

# An anchored regex pattern in the target ignore file must be detected by
# `dfm ignore <path>` as "already ignored" — no duplicate entry is appended.
#
# Regression: the check used to compute the relative path against the ignore
# file itself and matched via substring, so anchored patterns never matched
# and the path was appended again.

dfm init dotfiles

TARGET_IGNORE="$XDG_STATE_HOME/dfm/ignore_file"

mkdir -p dir
write "content" dir/file.txt

# pre-seed the ignore file with an anchored pattern
write '^dir/file\.txt$' "$TARGET_IGNORE"

dfm ignore dir/file.txt

# anchored pattern is kept, no duplicate appended
LINE_COUNT=$(wc -l <"$TARGET_IGNORE")
assert "1" = "$LINE_COUNT"
assert_succ grep -qF '^dir/file\.txt$' "$TARGET_IGNORE"

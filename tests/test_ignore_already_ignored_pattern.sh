# An anchored pattern added with --patterns must be detected by
# `dfm ignore <path>` as "already ignored" — no duplicate entry is appended.

dfm init dotfiles

TARGET_IGNORE="$XDG_STATE_HOME/dfm/ignore_file"

write "content" app.log

dfm ignore --patterns '^app\.log$'

dfm ignore app.log

# the anchored pattern is kept, no duplicate appended
LINE_COUNT=$(wc -l <"$TARGET_IGNORE")
assert "1" = "$LINE_COUNT"
assert_succ grep -qF '^app\.log$' "$TARGET_IGNORE"

# A source-side path already listed in the source ignore file must be detected
# by `dfm ignore <source-path>` as "already ignored" — no duplicate entry is
# appended to the source ignore file.

dfm init dotfiles

SOURCE_IGNORE="$PWD/dotfiles/.dfm_ignore_file"

# pre-seed the source ignore file with an anchored path
echo '^dot_my\.log$' >"$SOURCE_IGNORE"

write "content" my.log
dfm add my.log

dfm ignore "dotfiles/dot_my.log"

# the anchored pattern is kept, no duplicate appended
LINE_COUNT=$(wc -l <"$SOURCE_IGNORE")
assert "1" = "$LINE_COUNT"
assert_succ grep -qF '^my\.log$' "$SOURCE_IGNORE"

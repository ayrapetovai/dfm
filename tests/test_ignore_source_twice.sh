# Ignoring the same source-side path twice must not append a duplicate entry:
# the canonical (target-file) form stored on the first run is detected as
# "already ignored" on the second run.

dfm init dotfiles

SOURCE_IGNORE="$PWD/dotfiles/.dfm_ignore_file"

write "content" my.log
dfm add my.log

dfm ignore "dotfiles/dot_my.log"

# canonical entry is stored once
assert_succ grep -qF '^my\.log$' "$SOURCE_IGNORE"

dfm ignore "dotfiles/dot_my.log"

# no duplicate appended on the second run
assert "1" = "$(grep -cF '^my\.log$' "$SOURCE_IGNORE")"

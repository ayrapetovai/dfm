# Full cycle: add a regex pattern, verify ignored files are skipped,
# remove the pattern, verify previously ignored file can be added.

dfm init dotfiles

TARGET_IGNORE="$HOME/.local/state/dfm/ignore_file"

# add a regex pattern to ignore all .txt files
dfm ignore --patterns '\.txt$'

# postcondition: pattern is present in the ignore file
grep -qF '\.txt$' "$TARGET_IGNORE"

# try to add a .txt file — should be silently ignored
write "content" notes.txt
dfm add notes.txt
assert_no_source "notes.txt"

# remove the pattern from the ignore file
dfm ignore --remove '\.txt$'

# postcondition: pattern is gone from the ignore file
assert_fail grep -qF '\.txt$' "$TARGET_IGNORE"

# now the .txt file should be addable
dfm add notes.txt
assert_source "notes.txt"

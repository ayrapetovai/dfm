# Full cycle: ignore a file path, verify add skips it,
# remove the path from ignore, verify add works again.

dfm init dotfiles

TARGET_IGNORE="$HOME/.local/state/dfm/ignore_file"

write "content" file.txt

# ignore the file path
dfm ignore file.txt

# postcondition: escaped path is present in the ignore file
grep -qF 'file\.txt' "$TARGET_IGNORE"

# try to add — should be ignored
dfm add file.txt
assert_no_source "file.txt"

# remove the path from the ignore file
dfm ignore --remove 'file\.txt'

# postcondition: path is gone from the ignore file
assert_fail grep -qF 'file\.txt' "$TARGET_IGNORE"

# now add should succeed
dfm add file.txt
assert_source "file.txt"

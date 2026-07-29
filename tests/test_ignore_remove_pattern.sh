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

# --- path-based ignore: unescaped name removal ---
# Bug scenario: `ignore file.txt` stores `file\.txt` (regex-escaped) in the
# ignore file.  `--remove "file.txt"` (without the backslash) must also match.
write "content" data.txt
dfm add data.txt
dfm ignore data.txt
grep -qF 'data\.txt' "$TARGET_IGNORE"

# remove by unescaped path — must match the escaped line in the file
dfm ignore --remove "data.txt"
assert_fail grep -qF 'data\.txt' "$TARGET_IGNORE"

# file should be addable again
dfm add data.txt
assert_source "data.txt"

# --- path with punctuation: dots and brackets ---
write "content" "my.config"
dfm add "my.config"
dfm ignore "my.config"
grep -qF 'my\.config' "$TARGET_IGNORE"

# remove by unescaped name with dot
dfm ignore --remove "my.config"
assert_fail grep -qF 'my\.config' "$TARGET_IGNORE"

dfm add "my.config"
assert_source "my.config"

write "content" "lib[1].so"
dfm add "lib[1].so"
dfm ignore "lib[1].so"
grep -qF 'lib\[1\]\.so' "$TARGET_IGNORE"

# remove by unescaped name with brackets and dot
dfm ignore --remove "lib[1].so"
assert_fail grep -qF 'lib\[1\]\.so' "$TARGET_IGNORE"

dfm add "lib[1].so"
assert_source "lib[1].so"

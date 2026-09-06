# H1 — ignore --dry-run must not create a missing ignore file on disk
# (neither the target-side nor the source-side one), and a real run still does.

dfm init dotfiles
echo "content" >file.txt
dfm add file.txt

TARGET_IGNORE="$HOME/.local/state/dfm/ignore_file"
SOURCE_IGNORE="$HOME/dotfiles/.dfm_ignore_file"

# dry-run of a target-side path: missing target ignore file stays missing
rm -f "$TARGET_IGNORE"
dfm ignore --dry-run file.txt
assert_fail test -e "$TARGET_IGNORE"

# dry-run of a pattern: missing target ignore file stays missing
rm -f "$TARGET_IGNORE"
dfm ignore --dry-run -p '\.txt$'
assert_fail test -e "$TARGET_IGNORE"

# dry-run of a source-side path: missing source ignore file stays missing
rm -f "$SOURCE_IGNORE"
dfm ignore --dry-run dotfiles/file.txt
assert_fail test -e "$SOURCE_IGNORE"

# a real (non-dry-run) command still creates both files
rm -f "$TARGET_IGNORE" "$SOURCE_IGNORE"
dfm ignore file.txt
dfm ignore dotfiles/file.txt
assert_succ test -f "$TARGET_IGNORE"
assert_succ test -f "$SOURCE_IGNORE"


# Newline separator bug: when the ignore file does not end with `\n`,
# appending a new pattern must add it on a fresh line, not merge with the last line.

dfm init dotfiles

TARGET_IGNORE="$HOME/.local/state/dfm/ignore_file"

# --- case 1: append path after file without trailing newline ---
printf 'existing\\.pattern' > "$TARGET_IGNORE"
dfm ignore file.txt
LINE_COUNT=$(wc -l < "$TARGET_IGNORE")
assert "2" = "$LINE_COUNT"
assert_succ grep -qF 'existing\.pattern' "$TARGET_IGNORE"
assert_succ grep -qF 'file\.txt' "$TARGET_IGNORE"

# --- case 2: append regex pattern after file without trailing newline ---
rm -f "$TARGET_IGNORE"
printf 'existing\\.pattern' > "$TARGET_IGNORE"
dfm ignore --patterns '\.log$'
LINE_COUNT=$(wc -l < "$TARGET_IGNORE")
assert "2" = "$LINE_COUNT"
assert_succ grep -qF 'existing\.pattern' "$TARGET_IGNORE"
assert_succ grep -qF '\.log$' "$TARGET_IGNORE"

# --- case 3: append path after file that already ends with newline (regression) ---
rm -f "$TARGET_IGNORE"
printf 'existing\\.pattern\n' > "$TARGET_IGNORE"
dfm ignore file.txt
LINE_COUNT=$(wc -l < "$TARGET_IGNORE")
assert "2" = "$LINE_COUNT"

# --- case 4: append to empty file ---
rm -f "$TARGET_IGNORE"
dfm ignore file.txt
LINE_COUNT=$(wc -l < "$TARGET_IGNORE")
assert "1" = "$LINE_COUNT"

# --- case 5: dry-run must not modify existing file ---
printf 'existing\\.pattern' > "$TARGET_IGNORE"
BEFORE=$(cat "$TARGET_IGNORE")
dfm ignore --dry-run extra.txt
AFTER=$(cat "$TARGET_IGNORE")
assert "$BEFORE" = "$AFTER"

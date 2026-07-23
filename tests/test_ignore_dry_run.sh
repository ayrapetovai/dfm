# I1 / J1a / K1a — dry run skips writing to ignore files
dfm init dotfiles

echo "content" > file.txt
dfm add file.txt

# --- dry-run with a path (J1a) ---
TARGET_IGNORE="$HOME/.local/state/dfm/ignore_file"
# ensure the target ignore file exists before checking size
touch "$TARGET_IGNORE"
SIZE_BEFORE=$(wc -c < "$TARGET_IGNORE")
dfm ignore --dry-run file.txt
SIZE_AFTER=$(wc -c < "$TARGET_IGNORE")
assert "$SIZE_BEFORE" = "$SIZE_AFTER"

# --- dry-run with a pattern (K1a) ---
SIZE_BEFORE=$(wc -c < "$TARGET_IGNORE")
dfm ignore --dry-run -p '\.txt$'
SIZE_AFTER=$(wc -c < "$TARGET_IGNORE")
assert "$SIZE_BEFORE" = "$SIZE_AFTER"

# verify that the real command (without dry-run) actually writes
dfm ignore file.txt
SIZE_AFTER=$(wc -c < "$TARGET_IGNORE")
assert "$SIZE_BEFORE" -lt "$SIZE_AFTER"

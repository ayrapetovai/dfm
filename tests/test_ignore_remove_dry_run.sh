# R1 — dry-run remove does not modify the ignore file

dfm init dotfiles

TARGET_IGNORE="$HOME/.local/state/dfm/ignore_file"

# add a pattern first
dfm ignore --patterns '\.txt$'

SIZE_BEFORE=$(wc -c < "$TARGET_IGNORE")

# dry-run remove — should not change the file
dfm ignore --remove '\.txt$' --dry-run
SIZE_AFTER=$(wc -c < "$TARGET_IGNORE")
assert "$SIZE_BEFORE" = "$SIZE_AFTER"

# postcondition: pattern is still present
grep -qF '\.txt$' "$TARGET_IGNORE"

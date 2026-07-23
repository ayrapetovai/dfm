# E1 — ignore a path that's already in the target ignore file
dfm init dotfiles

write "content" file.txt
dfm add file.txt

# first ignore adds the path — 1 line in ignore file
dfm ignore file.txt

# second ignore detects it's already present and skips with "nothing to do"
dfm ignore file.txt

# target ignore file should still have exactly 1 line (no duplicate)
TARGET_IGNORE="$HOME/.local/state/dfm/ignore_file"
LINE_COUNT=$(wc -l < "$TARGET_IGNORE")
assert "1" = "$LINE_COUNT"

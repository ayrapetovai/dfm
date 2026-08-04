# A target path component literally starting with the dot prefix (e.g.
# `dot_backup`) must round-trip through the source namespace without being
# corrupted into a hidden file (`.backup`). The dot-prefix encoding must be
# injective: `.backup/dot_backup` -> `dot_backup/~dot_backup`.

dfm init dotfiles

CONTENT="$(uuid)"

mkdir -p "$PWD/.backup/dot_backup"
write "$CONTENT" ".backup/dot_backup/notes.txt"

dfm add .backup/dot_backup

# The literal `dot_` component is escaped in the source namespace...
assert_source "dot_backup/~dot_backup/notes.txt"

# ...and status reports the real target path, not a corrupted `.backup`.
STATUS_ALL=$(dfm status --all 2>/dev/null)
printf '%s\n' "$STATUS_ALL" | grep -qF -- "--  .backup/dot_backup/notes.txt"
printf '%s\n' "$STATUS_ALL" | grep -qF -- ".backup/dot_backup/notes.txt"
assert_fail grep -qF -- ".backup/.backup/notes.txt" <<<"$STATUS_ALL"

# After the target is removed the file is Unpulled at the correct path.
rm -rf "$PWD/.backup"
dfm status --short | grep -qF -- "!? .backup/dot_backup/notes.txt"
assert_fail grep -qF -- ".backup/.backup/notes.txt" <<<"$(dfm status --short)"

# Round trip: pulling from source restores the exact target layout.
dfm pull
assert -f "$PWD/.backup/dot_backup/notes.txt"
assert_content_eq "$PWD/.backup/dot_backup/notes.txt" "$CONTENT"
assert_fail test -e "$PWD/.backup/.backup/notes.txt"

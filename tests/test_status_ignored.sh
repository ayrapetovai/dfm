# Status: ignore patterns, stale patterns, ignored files
# Note: ignore patterns live in XDG state dir (~/.local/share/dfm/ignore_file),
# not in .dfm-ignore. Use `dfm ignore -p` to add patterns.

dfm init dotfiles

write "a" "managed.txt"
dfm add managed.txt

# Create an unmanaged file in a subdirectory
mkdir ignored_dir
write "b" "ignored_dir/file.txt"

# Add an ignore pattern for ignored_dir
dfm ignore -p "ignored_dir/"

# Default: ignored_dir not shown (only changes + unmanaged)
! dfm status 2>/dev/null | grep -q "ignored_dir"

# --ignored: show only ignored files
dfm status --ignored 2>/dev/null | grep -q "ignored_dir"

# --all: also shows ignored
dfm status --all 2>/dev/null | grep -q "ignored_dir"

# Short format with --all
dfm status --short --all 2>/dev/null | grep -qF "!! ignored_dir/"

# --ignored-patterns: list active ignore patterns
dfm status --ignored-patterns | grep -q "ignored_dir"

# --unused-patterns: stale pattern that matches nothing
dfm ignore -p "/nonexistent/.*"
dfm status --unused-patterns 2>/dev/null | grep -qF "!P"

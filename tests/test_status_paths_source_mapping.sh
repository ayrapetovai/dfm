# Status: a path inside the source directory is treated as a source path and
# mapped back to its target counterpart (like `pull` does), so users can ask
# about a managed file by either of its two locations.

dfm init dotfiles

mkdir -p .config/a .config/b docs
write "x" ".config/a/f.txt"
write "y" ".config/b/g.txt"
write "z" "docs/readme.txt"
write "top" "rootfile.txt"

# Managed file, then modified on target side
dfm add .config/a/f.txt
write "modified" ".config/a/f.txt"

# --- Source-file path maps to the target it backs ---------------------------
# A source-dir file refers to the managed file it backs; status reports the
# target-side entry (same as the full report does).
RES=$(dfm status "$PWD/dotfiles/dot_config/a/f.txt" 2>/dev/null)
echo "$RES" | grep -qF ".config/a/f.txt"
! echo "$RES" | grep -qF "rootfile.txt"

# --- Source directory + multiple source paths union -------------------------
dfm add docs/readme.txt
write "modified_docs" "docs/readme.txt"
RES=$(dfm status "$PWD/dotfiles/dot_config/a" "$PWD/dotfiles/docs/readme.txt" 2>/dev/null)
echo "$RES" | grep -qF ".config/a/f.txt"
echo "$RES" | grep -qF "docs/readme.txt"
! echo "$RES" | grep -qF "rootfile.txt"
! echo "$RES" | grep -qF ".config/b/g.txt"

# --- Source single file works with flags as well ----------------------------
write "modified_b" ".config/b/g.txt"
dfm add .config/b/g.txt
write "mod2" ".config/b/g.txt"
dfm status --porcelain "$PWD/dotfiles/dot_config/b/g.txt" 2>/dev/null | grep -qF "M 	.config/b/g.txt"

# Flags apply to source paths as well.
dfm status --porcelain "$PWD/dotfiles/dot_config/a/f.txt" 2>/dev/null | grep -qF "M 	.config/a/f.txt"
# Status flag filters: --modified, --unmanaged, --all

dfm init dotfiles

# --- setup: create files in different states ---
# BothModified
write "a" "both.txt"
dfm add both.txt
write "b" "both.txt"
write "c" "$PWD/dotfiles/both.txt"

# TargetModified (M )
write "d" "target_only.txt"
dfm add target_only.txt
write "e" "target_only.txt"

# SourceModified ( M)
write "f" "source_mod.txt"
dfm add source_mod.txt
write "g" "$PWD/dotfiles/source_mod.txt"

# Up to date (--)
write "h" "uptodate.txt"
dfm add uptodate.txt

# Unmanaged (??)
write "i" "unmanaged.txt"

# --- test --modified (any M variant) ---
dfm status --modified 2>/dev/null | grep -q "both.txt"
dfm status --modified 2>/dev/null | grep -q "target_only.txt"
dfm status --modified 2>/dev/null | grep -q "source_mod.txt"
# Should NOT show uptodate or unmanaged
! dfm status --modified 2>/dev/null | grep -q "uptodate.txt"
! dfm status --modified 2>/dev/null | grep -q "unmanaged.txt"

# --- test --unmanaged ---
dfm status --unmanaged 2>/dev/null | grep -q "unmanaged.txt"
! dfm status --unmanaged 2>/dev/null | grep -q "both.txt"

# --- test --all ---
dfm status --all 2>/dev/null | grep -qF -- "--  uptodate.txt"

# --- test --managed ---
# Shows managed files (both.txt, target_only.txt, source_mod.txt, uptodate.txt)
dfm status --managed 2>/dev/null | grep -q "both.txt"
dfm status --managed 2>/dev/null | grep -q "target_only.txt"
dfm status --managed 2>/dev/null | grep -q "source_mod.txt"
dfm status --managed 2>/dev/null | grep -q "uptodate.txt"
# Should NOT show unmanaged
! dfm status --managed 2>/dev/null | grep -q "unmanaged.txt"

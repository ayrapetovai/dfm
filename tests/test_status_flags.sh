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
RES=$(dfm status --modified 2>/dev/null)
assert_succ grep -q "both.txt" <<<"$RES"
assert_succ grep -q "target_only.txt" <<<"$RES"
assert_succ grep -q "source_mod.txt" <<<"$RES"
# Should NOT show uptodate or unmanaged
assert_fail grep -q "uptodate.txt" <<<"$RES"
assert_fail grep -q "unmanaged.txt" <<<"$RES"

# --- test --unmanaged ---
RES=$(dfm status --unmanaged 2>/dev/null)
assert_succ grep -q "unmanaged.txt" <<<"$RES"
assert_fail grep -q "both.txt" <<<"$RES"

# --- test --all ---
dfm status --all 2>/dev/null | grep -qF -- "--  uptodate.txt"

# --- test --managed ---
# Shows managed files (both.txt, target_only.txt, source_mod.txt, uptodate.txt)
RES=$(dfm status --managed 2>/dev/null)
assert_succ grep -q "both.txt" <<<"$RES"
assert_succ grep -q "target_only.txt" <<<"$RES"
assert_succ grep -q "source_mod.txt" <<<"$RES"
assert_succ grep -q "uptodate.txt" <<<"$RES"
# Should NOT show unmanaged
assert_fail grep -q "unmanaged.txt" <<<"$RES"

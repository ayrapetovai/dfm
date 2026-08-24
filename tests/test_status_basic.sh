# Status basic: empty, clean, dirty states

dfm init dotfiles

# this test needs a quiet tree; the unmanaged dfm config is out of scope here
dfm ignore .config/dfm

# Unmanaged file
write "content" "new.txt"

RES=$(dfm status)
assert_succ grep -qF "Unmanaged files" <<<"$RES"
assert_succ grep -qF "??  new.txt" <<<"$RES"
dfm status --short | assert_succ grep -q "?? new.txt"
dfm status --porcelain | assert_succ grep -q "??	new.txt"

# Exit code for unmanaged is 0
dfm status >/dev/null 2>&1

rm new.txt

# Clean state
write "hello" "clean.txt"
dfm add clean.txt
assert_source "clean.txt"

# Default: nothing actionable -> "All up-to-date." (exit 0)
dfm status | assert_succ grep -q "All up-to-date"
dfm status >/dev/null 2>&1

# --all: show -- entries
dfm status --all 2>/dev/null | assert_succ grep -qF -- "--  clean.txt"

# --all exit code is still 0 (no conflicts)
dfm status --all >/dev/null 2>&1

# BothModified -> exit code 0
write "modified_target" "clean.txt"
write "modified_source" "$PWD/dotfiles/clean.txt"

dfm status >/dev/null 2>&1
RES=$(dfm status)
assert_succ grep -qF "Changes to merge" <<<"$RES"
assert_succ grep -qF "MM  clean.txt" <<<"$RES"
dfm status --short | assert_succ grep -q "^MM clean.txt$"
dfm status --porcelain | assert_succ grep -q "^MM	clean.txt$"

# --conflicted filter
dfm status --conflicted 2>/dev/null | assert_succ grep -q "clean.txt"

# Exit code with conflicts is still 0 (MM surfaces via output/--conflicted,
# not the exit code — the old check claimed "is 1" but asserted -eq 0)
dfm status >/dev/null 2>&1

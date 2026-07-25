# Status basic: empty, clean, dirty states

dfm init dotfiles

# Unmanaged file
write "content" "new.txt"

dfm status | grep -q "Unmanaged files"
dfm status | grep -q "??  new.txt"
dfm status --short | grep -q "?? new.txt"
dfm status --porcelain | grep -q "??	new.txt"

# Exit code for unmanaged is 0 (only MM produces non-zero)
dfm status > /dev/null 2>&1

rm new.txt

# Clean state
write "hello" "clean.txt"
dfm add clean.txt
assert_source "clean.txt"

# Default: nothing actionable -> "Up to date" (exit 0)
dfm status | grep -q "Up to date"
dfm status > /dev/null 2>&1

# --all: show -- entries
dfm status --all 2>/dev/null | grep -qF -- "--  clean.txt"

# --all exit code is still 0 (no conflicts)
dfm status --all > /dev/null 2>&1

# BothModified -> exit code 1
write "modified_target" "clean.txt"
write "modified_source" "$PWD/dotfiles/clean.txt"

! dfm status > /dev/null 2>&1
dfm status 2>/dev/null | grep -q "Changes to merge"
dfm status 2>/dev/null | grep -q "MM  clean.txt"
dfm status --short | grep -q "^MM clean.txt$"
dfm status --porcelain | grep -q "^MM	clean.txt$"

# --conflicted filter
dfm status --conflicted 2>/dev/null | grep -q "clean.txt"

# Exit code with conflicts is 1
dfm_status_exit=0
dfm status > /dev/null 2>&1 || dfm_status_exit=$?
[ $dfm_status_exit -eq 1 ]

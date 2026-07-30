# Source file deleted — state entry is stale, file shows as unmanaged

dfm init dotfiles

# setup: add two files
write "a" "still_managed.txt"
write "b" "source_gone.txt"
dfm add still_managed.txt source_gone.txt

# delete one source file
rm "$PWD/dotfiles/source_gone.txt"

# file with source intact → --  (up to date) with --all
dfm status --all 2>/dev/null | grep -qF -- "--  still_managed.txt"

# file with deleted source → ?? (unmanaged), not NM
dfm status --all 2>/dev/null | grep -qF -- "??  source_gone.txt"

# must NOT show NM
! dfm status --all 2>/dev/null | grep -q "NM"

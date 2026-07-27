# Status: directory collapsing in status output
# Files sharing a common directory should be collapsed to {dir}/*
# instead of listing each file individually.

dfm init dotfiles

# Create unmanaged files in a shared directory (deep prefix)
mkdir -p snap/dir1
write "b" "snap/dir1/b.txt"
write "c" "snap/a.txt"

# They should collapse to snap/*
dfm status --all 2>/dev/null | grep -qF "??  snap/*"

# The individual files should NOT appear
! dfm status --all 2>/dev/null | grep -qF "snap/dir1/b.txt"
! dfm status --all 2>/dev/null | grep -qF "snap/a.txt"

# Create a second directory group
mkdir -p other
write "d" "other/x.txt"
write "e" "other/y.txt"

# Both groups collapse independently
dfm status --all 2>/dev/null | grep -qF "??  other/*"
! dfm status --all 2>/dev/null | grep -qF "other/x.txt"
! dfm status --all 2>/dev/null | grep -qF "other/y.txt"

# Root-level files (no '/') stay as-is
write "root" "root_file.txt"
dfm status --all 2>/dev/null | grep -qF "??  root_file.txt"

# Deep prefix: a/b/1.txt, a/b/2.txt, a/c/3.txt
# should collapse deepest first: a/b/*, then propagate to a/*
rm -rf snap other root_file.txt
mkdir -p a/b a/c
write "1" "a/b/1.txt"
write "2" "a/b/2.txt"
write "3" "a/c/3.txt"

dfm status --all 2>/dev/null | grep -qF "??  a/*"
! dfm status --all 2>/dev/null | grep -qF "a/b/1.txt"
! dfm status --all 2>/dev/null | grep -qF "a/b/2.txt"
! dfm status --all 2>/dev/null | grep -qF "a/c/3.txt"

# Single file in a subdirectory should NOT collapse
rm -rf a
mkdir -p lone
write "f" "lone/single.txt"
dfm status --all 2>/dev/null | grep -qF "lone/single.txt"
! dfm status --all 2>/dev/null | grep -qF "lone/*"

# Ignored files in a common directory also collapse
rm -rf lone
mkdir -p ignored_collapse
write "g" "ignored_collapse/file.txt"
write "h" "ignored_collapse/other.txt"
dfm ignore -p "ignored_collapse/"
dfm status --ignored 2>/dev/null | grep -qF "!!  ignored_collapse/*"


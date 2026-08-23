# Status: directory collapsing in status output
# Files sharing a common directory should be collapsed to {dir}/*
# instead of listing each file individually.

dfm init dotfiles

# Create unmanaged files in a shared directory (deep prefix)
mkdir -p snap/dir1
write "b" "snap/dir1/b.txt"
write "c" "snap/a.txt"

# They should collapse to snap/*
RES=$(dfm status --all 2>/dev/null)
assert_succ grep -qF "??  snap/*" <<<"$RES"

# The individual files should NOT appear
assert_fail grep -qF "snap/dir1/b.txt" <<<"$RES"
assert_fail grep -qF "snap/a.txt" <<<"$RES"

# Create a second directory group
mkdir -p other
write "d" "other/x.txt"
write "e" "other/y.txt"

# Both groups collapse independently
RES=$(dfm status --all 2>/dev/null)
assert_succ grep -qF "??  other/*" <<<"$RES"
assert_fail grep -qF "other/x.txt" <<<"$RES"
assert_fail grep -qF "other/y.txt" <<<"$RES"

# Root-level files (no '/') stay as-is
write "root" "root_file.txt"
RES=$(dfm status --all 2>/dev/null)
assert_succ grep -qF "??  root_file.txt" <<<"$RES"

# Deep prefix: a/b/1.txt, a/b/2.txt, a/c/3.txt
# should collapse deepest first: a/b/*, then propagate to a/*
rm -rf snap other root_file.txt
mkdir -p a/b a/c
write "1" "a/b/1.txt"
write "2" "a/b/2.txt"
write "3" "a/c/3.txt"

RES=$(dfm status --all 2>/dev/null)
assert_succ grep -qF "??  a/*" <<<"$RES"
assert_fail grep -qF "a/b/1.txt" <<<"$RES"
assert_fail grep -qF "a/b/2.txt" <<<"$RES"
assert_fail grep -qF "a/c/3.txt" <<<"$RES"

# Single file in a subdirectory should NOT collapse
rm -rf a
mkdir -p lone
write "f" "lone/single.txt"
RES=$(dfm status --all 2>/dev/null)
assert_succ grep -qF "lone/single.txt" <<<"$RES"
assert_fail grep -qF "lone/*" <<<"$RES"

# Fully-ignored directories are pruned during the walk and rendered as a
# single `!! dir/` entry (git-style), not enumerated file-by-file.
rm -rf lone
mkdir -p ignored_collapse
write "g" "ignored_collapse/file.txt"
write "h" "ignored_collapse/other.txt"
dfm ignore -p "ignored_collapse"
RES=$(dfm status --ignored 2>/dev/null)
assert_succ grep -qF "!!  ignored_collapse/" <<<"$RES"
RES=$(dfm status --ignored --short 2>/dev/null)
assert_fail grep -qF "file.txt" <<<"$RES"
assert_fail grep -qF "other.txt" <<<"$RES"

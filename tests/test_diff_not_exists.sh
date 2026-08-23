# A path that exists neither in the target directory nor in the source
# directory prints "<path> does not exist"; "is not managed" is reserved for
# files that exist but have no source.

dfm init dotfiles

# nonexistent target-side path with no source
RES=$(dfm diff ghost.txt 2>/dev/null)
assert_succ grep -qF "ghost.txt does not exist" <<<"$RES"
assert_fail grep -qF "is not managed" <<<"$RES"

# nonexistent source-side path whose corresponding target does not exist either
RES=$(dfm diff dotfiles/dot_ghost.txt 2>/dev/null)
assert_succ grep -qF "dotfiles/dot_ghost.txt does not exist" <<<"$RES"
assert_fail grep -qF "is not managed" <<<"$RES"

# contrast: missing source path whose target exists but is unmanaged
# (target "unmanaged.txt" maps to source "unmanaged.txt", no dot prefix)
write "orphan" unmanaged.txt
RES=$(dfm diff dotfiles/unmanaged.txt 2>/dev/null)
assert_succ grep -qF "dotfiles/unmanaged.txt is not managed" <<<"$RES"
assert_fail grep -qF "does not exist" <<<"$RES"

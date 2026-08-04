# Status: symlink with dot-prefixed name pointing outside the target directory.
#
# Verifies the fix for the bug where such symlinks produced garbage source
# paths like 'dot_/../../../file.txt' and/or crashed the status command
# (regression case: the pointee path is outside the target dir so
#  file_path_relative_to returns a path with ../../ components, and the
#  remove_dots_from_path function must handle these gracefully).
#
# Also covers:
#   ?L — unmanaged dot-prefixed symlink
#   LL — after dfm add (pointer-file-managed)
#   ?L — after dfm forget (reappears as unmanaged)
#   non-dot-prefixed symlink (sanity check)
#   broken dot-prefixed symlink where canonicalize fails

dfm init dotfiles

# ------------------------------------------------------------------
# Setup: create a file outside the target directory to be the pointee
# ------------------------------------------------------------------
mkdir -p "$PWD/../outside"
write "pointee_content" "$PWD/../outside/.alien_pointee"

# ------------------------------------------------------------------
# ?L: unmanaged dot-prefixed symlink pointing outside target dir
# ------------------------------------------------------------------
ln -s "$PWD/../outside/.alien_pointee" ".alien_link"

# Default output
dfm status 2>/dev/null | grep -qF "?L  .alien_link"

# --short
dfm status --short 2>/dev/null | grep -q "^?L .alien_link$"

# --porcelain
dfm status --porcelain 2>/dev/null | grep -q "^\?L	\.alien_link$"

# --unmanaged includes ?L
dfm status --unmanaged 2>/dev/null | grep -qF "?L  .alien_link"

# This is not in state — default output should mention unmanaged files too
dfm status 2>/dev/null | grep -qF "Unmanaged files"

# LL: after dfm add, the symlink is managed via a .symlink pointer file
dfm add .alien_link

# Source file is a symlink pointer (not a content copy)
assert_source "dot_alien_link.symlink"
assert_content_eq "$PWD/dotfiles/dot_alien_link.symlink" "$PWD/../outside/.alien_pointee"

# Status (default) should show "All up-to-date."
dfm status 2>/dev/null | grep -q "All up-to-date"

# --all should show LL
dfm status --all 2>/dev/null | grep -qF "LL  .alien_link"
dfm status --short --all 2>/dev/null | grep -q "^LL .alien_link$"
dfm status --porcelain --all 2>/dev/null | grep -q "^LL	\.alien_link$"

# ?L again: after dfm forget, symlink is no longer in state
dfm forget .alien_link
assert_no_source "dot_alien_link.symlink"

# Symlink still exists in the target dir
[ -L ".alien_link" ]

# Status should show ?L again
dfm status --short 2>/dev/null | grep -q "^?L .alien_link$"

# Non-dot-prefixed symlink for comparison
ln -s "$PWD/../outside/.alien_pointee" "plain_link"

dfm status --short 2>/dev/null | grep -q "^?L plain_link$"
dfm status --porcelain 2>/dev/null | grep -q "^\?L	plain_link$"
dfm status 2>/dev/null | grep -qF "?L  plain_link"

rm -f plain_link

# Broken symlink pointing outside (canonicalize fails)
ln -s "$PWD/../outside/does_not_exist" ".broken_link"
dfm status --short 2>/dev/null | grep -q "^?L .broken_link$"
rm -f .broken_link

# Cleanup
rm -f .alien_link
rm -rf "$PWD/../outside"

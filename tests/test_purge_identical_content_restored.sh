# purge must use the same change detection as add/pull (content hash for plain
# files, mtime for encrypted), not a raw mtime comparison.
#
# Case 1: identical content with a newer mtime is NOT an un-pulled change and
# must not block purge.
# Case 2: different content with a preserved mtime IS an un-pulled change and
# must block purge.

CONTENT="$(uuid)"

dfm init dotfiles
write "$CONTENT" file.txt
dfm add file.txt

# Case 1: restore identical content, bump the mtime
sleep 1
write "$CONTENT" "$PWD/dotfiles/file.txt"
dfm purge
assert_fail test -d "$PWD/dotfiles"

# re-set-up for case 2
dfm init dotfiles
write "$CONTENT" file.txt
dfm add file.txt

# Case 2: different content, preserve the mtime
ORIG_MTIME=$(stat -c %y "$PWD/dotfiles/file.txt")
write "changed" "$PWD/dotfiles/file.txt"
touch -d "$ORIG_MTIME" "$PWD/dotfiles/file.txt"

assert_fail dfm purge 2>/dev/null
assert -d "$PWD/dotfiles"

# --force still works
dfm purge --force
assert_fail test -d "$PWD/dotfiles"

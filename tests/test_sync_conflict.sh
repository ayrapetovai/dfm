# A BothModified conflict blocks the whole sync run unless --force is given.
# Without --force nothing is modified and sync exits non-zero.
# With --force the non-conflicting files still sync, the conflicting one is left alone.

ORIGINAL="$(uuid)"
TARGET_MOD="$(uuid)"
SOURCE_MOD="$(uuid)"
OTHER_TARGET="$(uuid)"
OTHER_SOURCE="$(uuid)"

dfm init dotfiles
write "$ORIGINAL" file.txt
write "$ORIGINAL" other.txt
dfm add file.txt other.txt

# make file.txt a conflict (both modified)
write "$TARGET_MOD" file.txt
write "$SOURCE_MOD" "$PWD/dotfiles/file.txt"
# make other.txt source-modified (only the source changed)
write "$OTHER_SOURCE" "$PWD/dotfiles/other.txt"

# without --force: conflict blocks everything, nothing modified, non-zero exit
assert_fail dfm sync
assert_content_eq "file.txt" "$TARGET_MOD"
assert_content_eq "$PWD/dotfiles/file.txt" "$SOURCE_MOD"
assert_content_eq "other.txt" "$ORIGINAL"
assert_content_eq "$PWD/dotfiles/other.txt" "$OTHER_SOURCE"

# with --force: non-conflicting other.txt syncs, conflicting file.txt untouched
dfm sync --force
assert_content_eq "other.txt" "$OTHER_SOURCE"
assert_content_eq "$PWD/dotfiles/other.txt" "$OTHER_SOURCE"
assert_content_eq "file.txt" "$TARGET_MOD"
assert_content_eq "$PWD/dotfiles/file.txt" "$SOURCE_MOD"

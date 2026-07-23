# B4b — BothModified: source and target both modified since last sync
CONTENT="$(uuid)"
MODIFIED_TARGET="$(uuid)"
MODIFIED_SOURCE="$(uuid)"

dfm init dotfiles
write "$CONTENT" file.txt
dfm add file.txt

# modify both target and source
write "$MODIFIED_TARGET" file.txt
write "$MODIFIED_SOURCE" "$PWD/dotfiles/file.txt"

# forget without --force must fail
assert_fail dfm forget file.txt
# nothing should be deleted
assert -f file.txt
assert_content_eq "file.txt" "$MODIFIED_TARGET"
assert_source "file.txt"
assert_content_eq "$PWD/dotfiles/file.txt" "$MODIFIED_SOURCE"

# forget with --force must succeed
dfm forget --force file.txt
# source file must be removed
assert_no_source "file.txt"
# target file must still exist with modified content
assert -f file.txt
assert_content_eq "file.txt" "$MODIFIED_TARGET"

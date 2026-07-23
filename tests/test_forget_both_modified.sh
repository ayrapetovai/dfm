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
assert "$MODIFIED_TARGET" = "$(cat file.txt)"
assert -f "$PWD/dotfiles/file.txt"
assert "$MODIFIED_SOURCE" = "$(cat "$PWD/dotfiles/file.txt")"

# forget with --force must succeed
dfm forget --force file.txt
# source file must be removed
assert_fail test -f "$PWD/dotfiles/file.txt"
# target file must still exist with modified content
assert -f file.txt
assert "$MODIFIED_TARGET" = "$(cat file.txt)"

ORIGINAL="$(uuid)"
MODIFIED="$(uuid)"

dfm init dotfiles
write "$ORIGINAL" file.txt
dfm add file.txt

# modify the target file
write "$MODIFIED" file.txt

# forget without --force must fail because target was modified
assert_fail dfm forget file.txt

# forget with --force must succeed
dfm forget --force file.txt

# postcondition: source file is removed
assert_fail test -f "$PWD/dotfiles/file.txt"

# target file must still exist
assert -f file.txt
assert "$MODIFIED" = "$(cat file.txt)"

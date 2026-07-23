ORIGINAL="$(uuid)"
TARGET_MOD="$(uuid)"
SOURCE_MOD="$(uuid)"

dfm init dotfiles
write "$ORIGINAL" file.txt
dfm add file.txt

# modify both target and source after the sync
write "$TARGET_MOD" file.txt
write "$SOURCE_MOD" "$PWD/dotfiles/file.txt"

# add without --force must detect conflict and fail
assert_fail dfm add file.txt

# postcondition: source still has its modified content (was not overwritten)
assert "$SOURCE_MOD" = "$(cat "$PWD/dotfiles/file.txt")"
# target still has its modified content
assert "$TARGET_MOD" = "$(cat file.txt)"

# add with --force must overwrite source with target content
dfm add -f file.txt

# postcondition: source now has the target's content
assert "$TARGET_MOD" = "$(cat "$PWD/dotfiles/file.txt")"

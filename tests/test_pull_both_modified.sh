ORIGINAL="$(uuid)"
TARGET_MOD="$(uuid)"
SOURCE_MOD="$(uuid)"

dfm init dotfiles
write "$ORIGINAL" file.txt
dfm add file.txt

# modify both target and source after the sync
write "$TARGET_MOD" file.txt
write "$SOURCE_MOD" "$PWD/dotfiles/file.txt"

# pull without --force must detect BothModified and fail
assert_fail dfm pull

# postcondition: target still has its modified content
assert "$TARGET_MOD" = "$(cat file.txt)"

# pull with --force must succeed and overwrite target with source
dfm pull --force

# postcondition: target now has the source's content
assert "$SOURCE_MOD" = "$(cat file.txt)"

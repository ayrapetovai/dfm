TARGET_CONTENT="$(uuid)"
SOURCE_CONTENT="$(uuid)"

dfm init dotfiles

# create both target and source files manually (bypassing add)
write "$TARGET_CONTENT" file.txt
write "$SOURCE_CONTENT" "$PWD/dotfiles/file.txt"

# pull without --force: source exists but has no sync record → should skip
dfm pull

# postcondition: target still has its original content (was not overwritten)
assert_content_eq "file.txt" "$TARGET_CONTENT"

# pull with --force: must overwrite target with source content
dfm pull --force

# postcondition: target now has source's content
assert_content_eq "file.txt" "$SOURCE_CONTENT"

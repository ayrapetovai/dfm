TARGET_CONTENT="$(uuid)"
SOURCE_CONTENT="$(uuid)"

dfm init dotfiles

# manually create both target and source files (bypassing add)
write "$TARGET_CONTENT" file.txt
mkdir -p "$PWD/dotfiles"
write "$SOURCE_CONTENT" "$PWD/dotfiles/file.txt"

# add without --force: source exists but has no sync record → should skip
dfm add file.txt

# postcondition: source still has its original content (was not overwritten)
assert_content_eq "$PWD/dotfiles/file.txt" "$SOURCE_CONTENT"

# add with --force: must overwrite source with target content
dfm add -f file.txt

# postcondition: source now has target's content
assert_content_eq "$PWD/dotfiles/file.txt" "$TARGET_CONTENT"

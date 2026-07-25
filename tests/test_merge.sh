# dfm merge (no paths) finds BothModified entries across the whole state
# and runs the merge tool on each.

CONTENT="$(uuid)"
MODIFIED="$(uuid)"
SOURCE_MODIFIED="$(uuid)"

dfm init dotfiles

# configure merge tool that keeps the target version (cp target → result)
dfm config --set merge_tool_command "cp {target} {result}"

# Setup: create a clean state for a plain file
write "$CONTENT" file.txt
dfm add file.txt
assert_source "file.txt"

# modify both target and source so they become BothModified
write "$MODIFIED" file.txt
write "$SOURCE_MODIFIED" "$PWD/dotfiles/file.txt"

# Act: dfm merge with no path arguments
dfm merge

# Assert: merge tool copied target → result → both sides
# Both should now contain $MODIFIED
assert_content_eq "file.txt" "$MODIFIED"
assert_content_eq "$PWD/dotfiles/file.txt" "$MODIFIED"

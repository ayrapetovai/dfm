# dfm merge PATH with a target path resolves the file via filepath_in_source_dir
# and finds the BothModified entry.

CONTENT="$(uuid)"
MODIFIED="$(uuid)"
SOURCE_MODIFIED="$(uuid)"

dfm init dotfiles
dfm config --set merge_tool_command "cp {target} {result}"

# ------------------------------------------------------------------
# Setup: create clean state
# ------------------------------------------------------------------
write "$CONTENT" file.txt
dfm add file.txt
assert_source "file.txt"

# modify both
write "$MODIFIED" file.txt
write "$SOURCE_MODIFIED" "$PWD/dotfiles/file.txt"

# ------------------------------------------------------------------
# Act: target-path branch — path is outside source_dir
# ------------------------------------------------------------------
dfm merge file.txt

# ------------------------------------------------------------------
# Assert: merge tool kept target version
# ------------------------------------------------------------------
assert_content_eq "file.txt" "$MODIFIED"
assert_content_eq "$PWD/dotfiles/file.txt" "$MODIFIED"

# Merge tool exits successfully (exit code 0) but does not create the result
# file — must be treated as an error.

CONTENT="$(uuid)"
MODIFIED="$(uuid)"

dfm init dotfiles

# Configure a merge tool that exits 0 but never touches the result file.
# `true` exits 0 and creates nothing — exactly the case we want to catch.
dfm config --set merge_tool_command "true"

# ------------------------------------------------------------------
# Setup: create a clean state for a plain file
# ------------------------------------------------------------------
write "$CONTENT" file.txt
dfm add file.txt
assert_source "file.txt"

# modify both target and source so they become BothModified
write "$MODIFIED" file.txt
write "$CONTENT" "$PWD/dotfiles/file.txt"

# ------------------------------------------------------------------
# Act: dfm merge — merge tool succeeds but writes no result
# ------------------------------------------------------------------
assert_fail dfm merge 2>/dev/null

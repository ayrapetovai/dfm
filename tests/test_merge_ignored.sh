# dfm merge skips files whose target path matches an ignore pattern.
# The ignore guard runs before the BothModified check.

CONTENT="$(uuid)"
MODIFIED="$(uuid)"
SOURCE_MODIFIED="$(uuid)"

dfm init dotfiles
dfm config --set merge_tool_command "cp {target} {result}"

# ------------------------------------------------------------------
# Setup: create clean state, then BothModify
# ------------------------------------------------------------------
write "$CONTENT" file.txt
dfm add file.txt
assert_source "file.txt"

write "$MODIFIED" file.txt
write "$SOURCE_MODIFIED" "$PWD/dotfiles/file.txt"

# add an ignore pattern that matches file.txt
dfm ignore --patterns 'file\.txt'

# ------------------------------------------------------------------
# Act: dfm merge should skip the ignored BothModified file
# ------------------------------------------------------------------
dfm merge

# ------------------------------------------------------------------
# Assert: neither side was merged — both retain their divergent content
# ------------------------------------------------------------------
assert_content_eq "file.txt" "$MODIFIED"
assert_content_eq "$PWD/dotfiles/file.txt" "$SOURCE_MODIFIED"

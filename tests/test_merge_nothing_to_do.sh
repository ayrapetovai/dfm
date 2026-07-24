# dfm merge when no entry is BothModified — all are NonModified or only
# one side was modified.  The candidate loop skips non-BothModified entries.
# The merge tool command uses a sentinel to prove it is never invoked.

CONTENT1="$(uuid)"
CONTENT2="$(uuid)"

dfm init dotfiles
dfm config --set merge_tool_command "touch $PWD/MERGE_TOOL_WAS_CALLED"

# ------------------------------------------------------------------
# Setup: add two files in a clean state — neither gets modified
# ------------------------------------------------------------------
write "$CONTENT1" foo.txt
dfm add foo.txt
assert_source "foo.txt"

write "$CONTENT2" bar.txt
dfm add bar.txt
assert_source "bar.txt"

# ------------------------------------------------------------------
# Act: both entries are NonModified → nothing to merge
# ------------------------------------------------------------------
dfm merge

# ------------------------------------------------------------------
# Assert: both files unchanged AND merge tool was never called
# ------------------------------------------------------------------
assert_content_eq "foo.txt" "$CONTENT1"
assert_content_eq "bar.txt" "$CONTENT2"
assert_content_eq "$PWD/dotfiles/foo.txt" "$CONTENT1"
assert_content_eq "$PWD/dotfiles/bar.txt" "$CONTENT2"
assert_fail test -f "$PWD/MERGE_TOOL_WAS_CALLED"

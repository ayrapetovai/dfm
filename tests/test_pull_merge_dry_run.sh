# dfm pull --merge --dry-run must not call the merge tool.

CONTENT="$(uuid)"
MODIFIED="$(uuid)"
SENTINEL="$PWD/MERGE_TOOL_WAS_CALLED"

dfm init dotfiles

# Configure a merge tool that touches a sentinel file.
dfm config --set merge_tool_command "touch $SENTINEL"

# ------------------------------------------------------------------
# Setup: create a clean state for a plain file
# ------------------------------------------------------------------
write "$CONTENT" file.txt
dfm add file.txt
assert_source "file.txt"

rm file.txt
dfm pull
assert_content_eq "file.txt" "$CONTENT"

# modify both target and source so they become BothModified
write "$MODIFIED" file.txt
write "$CONTENT" "$PWD/dotfiles/file.txt"

# ------------------------------------------------------------------
# Act: dry-run pull --merge — the merge tool must never run
# ------------------------------------------------------------------
dfm pull --merge --dry-run file.txt

# ------------------------------------------------------------------
# Assert: sentinel file must not exist
# ------------------------------------------------------------------
assert_fail test -f "$SENTINEL"

# ------------------------------------------------------------------
# Also test the global --dry-run flag (-n)
# ------------------------------------------------------------------
dfm pull --merge -n file.txt
assert_fail test -f "$SENTINEL"

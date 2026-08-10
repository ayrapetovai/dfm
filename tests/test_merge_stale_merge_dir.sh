# A stale `.current_merge/` (e.g. left by a killed merge tool) must be removed
# before starting a new merge. Otherwise a leftover `result.*` file from the
# stale directory would make dfm believe the merge tool produced output when it
# did not.

CONTENT="$(uuid)"
MODIFIED="$(uuid)"
STALE="stale-result-content"

dfm init dotfiles

# merge tool exits 0 but creates nothing
dfm config --set merge_tool_command "true"

# Setup: clean state for a plain file
write "$CONTENT" file.txt
dfm add file.txt
assert_source "file.txt"

# BothModified, so a merge is needed
write "$MODIFIED" file.txt
write "$MODIFIED" "$PWD/dotfiles/file.txt"

# Simulate a killed merge tool: a stale .current_merge with a leftover
# result file that would be mistaken for a fresh merge output.
mkdir -p "$PWD/dotfiles/.current_merge"
write "$STALE" "$PWD/dotfiles/.current_merge/result.file.txt"

# Act: dfm merge — tool writes no result, so it must fail even though a stale
# result file exists.
assert_fail dfm merge 2>/dev/null

# The stale result content must not have been written to either side.
assert_content_eq "file.txt" "$MODIFIED"
assert_content_eq "$PWD/dotfiles/file.txt" "$MODIFIED"


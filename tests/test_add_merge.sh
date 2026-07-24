# dfm add --merge calls the merge tool on BothModified/SourceModified conflicts,
# then syncs the merged result to the source.

CONTENT="$(uuid)"
MODIFIED="$(uuid)"

dfm init dotfiles

# configure a merge tool that copies the incoming changes to the result file
dfm config --set merge_tool_command "cp {target} {result}"

# ------------------------------------------------------------------
# Scenario 1: BothModified — add --merge resolves via merge tool
# ------------------------------------------------------------------
write "$CONTENT" file.txt
dfm add file.txt
assert_source "file.txt"
assert_content_eq "$PWD/dotfiles/file.txt" "$CONTENT"

# modify both target and source
write "$MODIFIED" file.txt
write "$CONTENT" "$PWD/dotfiles/file.txt"

# add --merge should call the merge tool and sync the result to source
dfm add --merge file.txt

# source was updated with merged content
assert_source "file.txt"

# ------------------------------------------------------------------
# Scenario 2: SourceModified — add --merge resolves via merge tool
# ------------------------------------------------------------------
# re-add to get a clean state
write "$CONTENT" file.txt
dfm add --force file.txt

# modify only source
write "$MODIFIED" "$PWD/dotfiles/file.txt"

# add --merge should merge and sync to source
dfm add --merge file.txt
assert_source "file.txt"

# ------------------------------------------------------------------
# Scenario 3: merge tool fails — error
# ------------------------------------------------------------------
write "$CONTENT" file.txt
dfm add --force file.txt
write "$MODIFIED" "$PWD/dotfiles/file.txt"

dfm config --set merge_tool_command "false"
assert_fail dfm add --merge file.txt 2>/dev/null

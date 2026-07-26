# `dfm merge` resolves BothModified conflicts that occur during `add`.
# (The --merge flag has been removed from `add`; use the standalone
# `merge` subcommand instead.)

CONTENT="$(uuid)"
MODIFIED="$(uuid)"

dfm init dotfiles

# configure a merge tool that copies the incoming changes to the result file
dfm config --set merge_tool_command "cp {target} {result}"

# Scenario 1: BothModified — dfm merge resolves via merge tool
write "$CONTENT" file.txt
dfm add file.txt
assert_source "file.txt"
assert_content_eq "$PWD/dotfiles/file.txt" "$CONTENT"

# modify both target and source
write "$MODIFIED" file.txt
write "$CONTENT" "$PWD/dotfiles/file.txt"

# standalone merge resolves the conflict and syncs result to both sides
dfm merge file.txt

# source was updated with merged content (merge tool kept target version)
assert_source "file.txt"
assert_content_eq "$PWD/dotfiles/file.txt" "$MODIFIED"

# Scenario 2: SourceModified — add without --force fails, with --force succeeds
# re-add to get a clean state
write "$CONTENT" file.txt
dfm add --force file.txt

# modify only source
write "$MODIFIED" "$PWD/dotfiles/file.txt"

# add without --force must fail with a conflict
assert_fail dfm add file.txt 2>/dev/null

# add with --force must overwrite source
dfm add --force file.txt
assert_source "file.txt"
assert_content_eq "$PWD/dotfiles/file.txt" "$CONTENT"

# Scenario 3: merge tool fails — error
write "$CONTENT" file.txt
dfm add --force file.txt

# modify both to create BothModified
write "$MODIFIED" file.txt
touch "$PWD/dotfiles/file.txt"

dfm config --set merge_tool_command "false"
assert_fail dfm merge file.txt 2>/dev/null

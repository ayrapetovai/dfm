# dfm merge with a path that is not tracked in state — warns and skips.
# Tests both the target-path branch and source-path branch with unknown paths.

CONTENT="$(uuid)"

dfm init dotfiles
dfm config --set merge_tool_command "cp {target} {result}"

# Scenario 1: target path not in state
write "$CONTENT" unknown.txt
dfm merge unknown.txt 2>/dev/null

# (no crash, no file created in source)
assert_no_source "unknown.txt"

# Scenario 2: source path not in state
write "$CONTENT" "$PWD/dotfiles/stranger.txt"
dfm merge "$PWD/dotfiles/stranger.txt" 2>/dev/null

# (no crash)

# Regression: forgetting a target that is a directory must remove the whole
# source subtree instead of failing with EISDIR (remove_file on a directory),
# and must clear every state entry living inside that subtree.
dfm init dotfiles

mkdir -p .local/share/opencode
write "cfg" .local/share/opencode/config.json
dfm add ".local/share/opencode"
assert_source "dot_local/share/opencode/config.json"

# forget by the directory target path
dfm forget ".local/share/opencode"

# source subtree must be gone, target must be intact
assert_fail test -d "$PWD/dotfiles/dot_local/share/opencode"
assert -f .local/share/opencode/config.json
assert_content_eq ".local/share/opencode/config.json" "cfg"

# state entries for the subtree must be cleared
grep -q "syncs" "$PWD/.local/state/dfm/state.toml" || true
if grep -q 'dot_local/share/opencode' "$PWD/.local/state/dfm/state.toml"; then
    echo "state entry for subtree not cleared"
    exit 1
fi

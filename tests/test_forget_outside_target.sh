# B5 — path outside both source and target directories
# The code warns and continues (exit 0, nothing to do)
dfm init dotfiles

OUTSIDE_FILE="$(mktemp --tmpdir=/tmp forget_outside_XXXX.txt)"
echo "outside" > "$OUTSIDE_FILE"

# forgetting a path outside management succeeds but does nothing
dfm forget "$OUTSIDE_FILE"

# outside file should remain untouched
assert -f "$OUTSIDE_FILE"
assert_content_eq "$OUTSIDE_FILE" "outside"
rm -f "$OUTSIDE_FILE"

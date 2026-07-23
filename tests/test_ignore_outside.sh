# F1 — ignore a path outside both source and target directories
dfm init dotfiles

OUTSIDE_FILE="$(mktemp --tmpdir=/tmp ignore_outside_XXXX.txt)"
echo "content" > "$OUTSIDE_FILE"

# ignoring a path outside management succeeds but does nothing
dfm ignore "$OUTSIDE_FILE"

# outside file should remain untouched
assert -f "$OUTSIDE_FILE"
rm -f "$OUTSIDE_FILE"

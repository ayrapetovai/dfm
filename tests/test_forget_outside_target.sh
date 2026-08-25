# B5 — path outside both source and target directories
# The scope check rejects it (exit != 0), nothing is forgotten.
dfm init dotfiles

OUTSIDE_FILE="$(mktemp --tmpdir=/tmp forget_outside_XXXX.txt)"
echo "outside" > "$OUTSIDE_FILE"

run_fail dfm forget "$OUTSIDE_FILE"
assert_succ grep -qF "outside the target directory" <<<"$FAIL_OUTPUT"

# outside file should remain untouched
assert -f "$OUTSIDE_FILE"
assert_content_eq "$OUTSIDE_FILE" "outside"
rm -f "$OUTSIDE_FILE"
